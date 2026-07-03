use axum::Json;
use axum::extract::{Path, Query};
use axum::http::StatusCode;
use chrono::{Datelike, NaiveDate};
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::dates::{business_days, current_year, is_weekend};
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

    let days: f64 = if day_part == "full" {
        f64::from(business_days(body.start_date, body.end_date))
    } else {
        0.5
    };
    if days == 0.0 {
        return Err(ApiError::Unprocessable(
            "range contains no business days".to_string(),
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

    let pool = db::pool().await?;
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
        if require_approval {
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

pub async fn remove(user: AuthUser, Path(id): Path<Uuid>) -> Result<StatusCode, ApiError> {
    let pool = db::pool().await?;
    let owner: Option<Uuid> = sqlx::query_scalar("SELECT user_id FROM absences WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?;

    let Some(owner) = owner else {
        return Err(ApiError::NotFound);
    };
    if owner != user.id && !user.is_admin() {
        return Err(ApiError::Forbidden);
    }

    sqlx::query("DELETE FROM absences WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn approve(user: AuthUser, Path(id): Path<Uuid>) -> Result<Json<Absence>, ApiError> {
    user.require_admin()?;
    let pool = db::pool().await?;
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
    let absence: Option<Absence> = sqlx::query_as(&format!(
        "UPDATE absences SET status = 'denied', decision_reason = $2
         WHERE id = $1 RETURNING {ABSENCE_COLUMNS}"
    ))
    .bind(id)
    .bind(reason)
    .fetch_optional(pool)
    .await?;
    absence.map(Json).ok_or(ApiError::NotFound)
}
