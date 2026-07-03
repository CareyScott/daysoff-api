use axum::Json;
use axum::extract::{Path, Query};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use chrono::{Datelike, NaiveDate};
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::dates::{current_year, is_weekend};
use crate::db;
use crate::error::ApiError;
use crate::models::{ABSENCE_COLUMNS, Absence};
use crate::routes::overview::{allowance_days, taken_days};

#[derive(Deserialize)]
pub struct ListQuery {
    pub year: Option<i32>,
    pub user_id: Option<Uuid>,
}

pub async fn list(
    _user: AuthUser,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<Absence>>, ApiError> {
    let year = q.year.unwrap_or_else(current_year);
    let pool = db::pool().await?;

    let absences: Vec<Absence> = sqlx::query_as(&format!(
        "SELECT {ABSENCE_COLUMNS}
         FROM absences
         WHERE date_part('year', start_date) = $1
           AND ($2::uuid IS NULL OR user_id = $2)
         ORDER BY start_date"
    ))
    .bind(f64::from(year))
    .bind(q.user_id)
    .fetch_all(pool)
    .await?;

    Ok(Json(absences))
}

#[derive(Deserialize)]
pub struct CreateBody {
    pub kind: String,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub user_id: Option<Uuid>,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub day_part: Option<String>,
}

pub async fn create(
    user: AuthUser,
    Json(body): Json<CreateBody>,
) -> Result<(StatusCode, Json<Absence>), ApiError> {
    if body.kind != "vacation" && body.kind != "sick" {
        return Err(ApiError::Unprocessable("kind must be 'vacation' or 'sick'".to_string()));
    }
    if body.end_date < body.start_date {
        return Err(ApiError::Unprocessable("end date is before start date".to_string()));
    }
    if body.start_date.year() != body.end_date.year() {
        return Err(ApiError::Unprocessable(
            "absence must stay within one calendar year".to_string(),
        ));
    }

    let day_part = body.day_part.as_deref().unwrap_or("full");
    if !matches!(day_part, "full" | "am" | "pm") {
        return Err(ApiError::Unprocessable("day part must be 'full', 'am' or 'pm'".to_string()));
    }
    if day_part != "full" {
        if body.start_date != body.end_date {
            return Err(ApiError::Unprocessable(
                "half days apply to a single date".to_string(),
            ));
        }
        if is_weekend(body.start_date) {
            return Err(ApiError::Unprocessable("that day is a weekend".to_string()));
        }
    }

    // Company days are already off for everyone: they cost nothing and a
    // booking that covers only company days / weekends is rejected.
    let pool = db::pool().await?;
    let company_rows: Vec<(NaiveDate, NaiveDate, String)> = sqlx::query_as(
        "SELECT start_date, end_date, day_part FROM company_days
         WHERE daterange(start_date, end_date, '[]') && daterange($1, $2, '[]')",
    )
    .bind(body.start_date)
    .bind(body.end_date)
    .fetch_all(pool)
    .await?;

    let mut coverage: std::collections::HashMap<NaiveDate, String> =
        std::collections::HashMap::new();
    for (cd_start, cd_end, cd_part) in &company_rows {
        let mut d = *cd_start;
        while d <= *cd_end {
            let entry = coverage.entry(d).or_insert_with(|| cd_part.clone());
            // Merge overlapping company days: full wins; am + pm = full.
            if entry != cd_part {
                *entry = "full".to_string();
            }
            d = d.succ_opt().expect("date overflow");
        }
    }

    let days: f64 = if day_part == "full" {
        let mut sum = 0.0;
        let mut d = body.start_date;
        while d <= body.end_date {
            if !is_weekend(d) {
                sum += match coverage.get(&d).map(String::as_str) {
                    Some("full") => 0.0,
                    Some(_) => 0.5,
                    None => 1.0,
                };
            }
            d = d.succ_opt().expect("date overflow");
        }
        sum
    } else {
        match coverage.get(&body.start_date).map(String::as_str) {
            Some("full") => 0.0,
            Some(part) if part == day_part => 0.0,
            Some(_) => 0.5,
            None => 0.5,
        }
    };
    if days == 0.0 {
        return Err(ApiError::Unprocessable(
            "these days are already off (weekends or company days)".to_string(),
        ));
    }

    let note = body
        .note
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(|n| {
            if n.len() > 500 {
                Err(ApiError::Unprocessable("note is too long (max 500 characters)".to_string()))
            } else {
                Ok(n.to_string())
            }
        })
        .transpose()?;

    // Members always book for themselves; user_id is only honored for admins.
    let target = match body.user_id {
        Some(id) if user.is_admin() => id,
        _ => user.id,
    };

    let year = body.start_date.year();

    // Overlap check covering half-day combinations (am + pm on the same day is
    // fine; anything else that shares a date conflicts). The DB exclusion
    // constraint remains the backstop for full-day races.
    let conflicts: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM absences
         WHERE user_id = $1 AND status <> 'denied'
           AND daterange(start_date, end_date, '[]') && daterange($2, $3, '[]')
           AND (day_part = 'full' OR $4 = 'full' OR day_part = $4)",
    )
    .bind(target)
    .bind(body.start_date)
    .bind(body.end_date)
    .bind(day_part)
    .fetch_one(pool)
    .await?;
    if conflicts > 0 {
        return Err(ApiError::Conflict("overlaps an existing absence".to_string()));
    }

    // Approval only gates vacations; sick days are always recorded directly.
    // Admins' own bookings (and bookings they make for others) skip approval.
    let mut status = "approved";
    if body.kind == "vacation" && !user.is_admin() {
        let require_approval: bool =
            sqlx::query_scalar("SELECT require_approval FROM settings WHERE id")
                .fetch_one(pool)
                .await?;
        // Backdated vacations ALWAYS need an admin, even when the workspace
        // does not require approval in general.
        let today = chrono::Utc::now().date_naive();
        if require_approval || body.start_date < today {
            status = "pending";
        }
    }

    if body.kind == "vacation" {
        let allowance = f64::from(allowance_days(pool, target, year).await?);
        let (vacation_taken, pending, _) = taken_days(pool, target, year).await?;
        if vacation_taken + pending + days > allowance {
            return Err(ApiError::Unprocessable(format!(
                "not enough vacation days left: {} requested, {} remaining",
                days,
                allowance - vacation_taken - pending
            )));
        }
    }

    let absence: Absence = sqlx::query_as(&format!(
        "INSERT INTO absences (user_id, kind, start_date, end_date, business_days, status, day_part, note)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         RETURNING {ABSENCE_COLUMNS}"
    ))
    .bind(target)
    .bind(&body.kind)
    .bind(body.start_date)
    .bind(body.end_date)
    .bind(days)
    .bind(status)
    .bind(day_part)
    .bind(&note)
    .fetch_one(pool)
    .await?;

    Ok((StatusCode::CREATED, Json(absence)))
}

pub async fn remove(user: AuthUser, Path(id): Path<Uuid>) -> Result<Response, ApiError> {
    let pool = db::pool().await?;
    let row: Option<(Uuid, String, String, NaiveDate)> = sqlx::query_as(
        "SELECT user_id, kind, status, start_date FROM absences WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    let Some((owner, kind, status, start_date)) = row else {
        return Err(ApiError::NotFound);
    };
    if owner != user.id && !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    // Members cannot silently remove a vacation that already started:
    // it becomes a cancellation request an admin has to approve.
    // (Their own still-pending requests and sick days delete directly.)
    let today = chrono::Utc::now().date_naive();
    if !user.is_admin()
        && kind == "vacation"
        && start_date < today
        && matches!(status.as_str(), "approved" | "cancel_pending")
    {
        let absence: Absence = sqlx::query_as(&format!(
            "UPDATE absences SET status = 'cancel_pending'
             WHERE id = $1 RETURNING {ABSENCE_COLUMNS}"
        ))
        .bind(id)
        .fetch_one(pool)
        .await?;
        return Ok((StatusCode::OK, Json(absence)).into_response());
    }

    sqlx::query("DELETE FROM absences WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;

    Ok(StatusCode::NO_CONTENT.into_response())
}

pub async fn approve(user: AuthUser, Path(id): Path<Uuid>) -> Result<Json<Absence>, ApiError> {
    user.require_admin()?;
    let pool = db::pool().await?;

    // Approving a cancellation request deletes the absence.
    let current: Option<String> = sqlx::query_scalar("SELECT status FROM absences WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    if current.as_deref() == Some("cancel_pending") {
        let absence: Absence = sqlx::query_as(&format!(
            "DELETE FROM absences WHERE id = $1 RETURNING {ABSENCE_COLUMNS}"
        ))
        .bind(id)
        .fetch_one(pool)
        .await?;
        return Ok(Json(absence));
    }

    let absence: Option<Absence> = sqlx::query_as(&format!(
        "UPDATE absences SET status = 'approved', decision_reason = NULL
         WHERE id = $1 RETURNING {ABSENCE_COLUMNS}"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await?;
    absence.map(Json).ok_or(ApiError::NotFound)
}

#[derive(Deserialize)]
pub struct DenyBody {
    pub reason: String,
}

pub async fn deny(
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<DenyBody>,
) -> Result<Json<Absence>, ApiError> {
    user.require_admin()?;
    let reason = body.reason.trim();
    if reason.is_empty() {
        return Err(ApiError::Unprocessable("a reason is required to deny a request".to_string()));
    }
    if reason.len() > 500 {
        return Err(ApiError::Unprocessable("reason is too long (max 500 characters)".to_string()));
    }

    let pool = db::pool().await?;

    // Denying a cancellation request keeps the vacation booked (approved),
    // with the reason recorded for the requester.
    let absence: Option<Absence> = sqlx::query_as(&format!(
        "UPDATE absences SET
            status = CASE WHEN status = 'cancel_pending' THEN 'approved' ELSE 'denied' END,
            decision_reason = $2
         WHERE id = $1 RETURNING {ABSENCE_COLUMNS}"
    ))
    .bind(id)
    .bind(reason)
    .fetch_optional(pool)
    .await?;
    absence.map(Json).ok_or(ApiError::NotFound)
}
