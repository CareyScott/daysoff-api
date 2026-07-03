use axum::Json;
use axum::extract::{Path, Query};
use axum::http::StatusCode;
use chrono::{Datelike, NaiveDate};
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::dates::{business_days, current_year};
use crate::db;
use crate::error::ApiError;
use crate::models::Absence;
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

    let absences: Vec<Absence> = sqlx::query_as(
        "SELECT id, user_id, kind, start_date, end_date, business_days
         FROM absences
         WHERE date_part('year', start_date) = $1
           AND ($2::uuid IS NULL OR user_id = $2)
         ORDER BY start_date",
    )
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
    let days = business_days(body.start_date, body.end_date);
    if days == 0 {
        return Err(ApiError::Unprocessable(
            "range contains no business days".to_string(),
        ));
    }

    // Members always book for themselves; user_id is only honored for admins.
    let target = match body.user_id {
        Some(id) if user.is_admin() => id,
        _ => user.id,
    };

    let pool = db::pool().await?;
    let year = body.start_date.year();

    if body.kind == "vacation" {
        let allowance = allowance_days(pool, target, year).await?;
        let (vacation_taken, _) = taken_days(pool, target, year).await?;
        if vacation_taken + days > allowance {
            return Err(ApiError::Unprocessable(format!(
                "not enough vacation days left: {} requested, {} remaining",
                days,
                allowance - vacation_taken
            )));
        }
    }

    let absence: Absence = sqlx::query_as(
        "INSERT INTO absences (user_id, kind, start_date, end_date, business_days)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING id, user_id, kind, start_date, end_date, business_days",
    )
    .bind(target)
    .bind(&body.kind)
    .bind(body.start_date)
    .bind(body.end_date)
    .bind(days)
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
