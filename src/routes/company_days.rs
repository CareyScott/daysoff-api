use axum::Json;
use axum::extract::{Path, Query};
use axum::http::StatusCode;
use chrono::NaiveDate;
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::dates::current_year;
use crate::db;
use crate::error::ApiError;
use crate::models::CompanyDay;

#[derive(Deserialize)]
pub struct YearQuery {
    pub year: Option<i32>,
}

pub async fn list(
    _user: AuthUser,
    Query(q): Query<YearQuery>,
) -> Result<Json<Vec<CompanyDay>>, ApiError> {
    let year = q.year.unwrap_or_else(current_year);
    let pool = db::pool().await?;
    let days: Vec<CompanyDay> = sqlx::query_as(
        "SELECT id, name, start_date, end_date, day_part FROM company_days
         WHERE date_part('year', start_date) = $1 OR date_part('year', end_date) = $1
         ORDER BY start_date",
    )
    .bind(f64::from(year))
    .fetch_all(pool)
    .await?;
    Ok(Json(days))
}

#[derive(Deserialize)]
pub struct CreateBody {
    pub name: String,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    #[serde(default)]
    pub day_part: Option<String>,
}

pub async fn create(
    user: AuthUser,
    Json(body): Json<CreateBody>,
) -> Result<(StatusCode, Json<CompanyDay>), ApiError> {
    user.require_admin()?;
    let name = body.name.trim();
    if name.is_empty() || name.len() > 100 {
        return Err(ApiError::Unprocessable("name must be 1-100 characters".to_string()));
    }
    if body.end_date < body.start_date {
        return Err(ApiError::Unprocessable("end date is before start date".to_string()));
    }
    let day_part = body.day_part.as_deref().unwrap_or("full");
    if !matches!(day_part, "full" | "am" | "pm") {
        return Err(ApiError::Unprocessable("day part must be 'full', 'am' or 'pm'".to_string()));
    }
    if day_part != "full" && body.start_date != body.end_date {
        return Err(ApiError::Unprocessable("half days apply to a single date".to_string()));
    }

    let pool = db::pool().await?;
    let day: CompanyDay = sqlx::query_as(
        "INSERT INTO company_days (name, start_date, end_date, day_part)
         VALUES ($1, $2, $3, $4)
         RETURNING id, name, start_date, end_date, day_part",
    )
    .bind(name)
    .bind(body.start_date)
    .bind(body.end_date)
    .bind(day_part)
    .fetch_one(pool)
    .await?;

    Ok((StatusCode::CREATED, Json(day)))
}

pub async fn remove(user: AuthUser, Path(id): Path<Uuid>) -> Result<StatusCode, ApiError> {
    user.require_admin()?;
    let pool = db::pool().await?;
    let result = sqlx::query("DELETE FROM company_days WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}
