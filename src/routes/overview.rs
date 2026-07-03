use axum::Json;
use axum::extract::Query;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::dates::current_year;
use crate::db;
use crate::error::ApiError;
use crate::models::{Absence, PublicUser, Summary};

#[derive(Deserialize)]
pub struct YearQuery {
    pub year: Option<i32>,
}

/// (approved vacation days, pending vacation days, sick days) for a year.
/// Denied requests count for nothing.
pub(crate) async fn taken_days(
    pool: &PgPool,
    user_id: Uuid,
    year: i32,
) -> Result<(f64, f64, f64), ApiError> {
    let (vacation, pending, sick): (f64, f64, f64) = sqlx::query_as(
        "SELECT
            COALESCE(SUM(business_days) FILTER (WHERE kind = 'vacation' AND status IN ('approved', 'cancel_pending')), 0)::float8,
            COALESCE(SUM(business_days) FILTER (WHERE kind = 'vacation' AND status = 'pending'), 0)::float8,
            COALESCE(SUM(business_days) FILTER (WHERE kind = 'sick' AND status <> 'denied'), 0)::float8
         FROM absences
         WHERE user_id = $1 AND date_part('year', start_date) = $2",
    )
    .bind(user_id)
    .bind(f64::from(year))
    .fetch_one(pool)
    .await?;
    Ok((vacation, pending, sick))
}

pub(crate) async fn allowance_days(
    pool: &PgPool,
    user_id: Uuid,
    year: i32,
) -> Result<i32, ApiError> {
    let days: Option<i32> =
        sqlx::query_scalar("SELECT days FROM allowances WHERE user_id = $1 AND year = $2")
            .bind(user_id)
            .bind(year)
            .fetch_optional(pool)
            .await?;
    Ok(days.unwrap_or(0))
}

pub(crate) async fn summary_for(
    pool: &PgPool,
    user_id: Uuid,
    year: i32,
) -> Result<Summary, ApiError> {
    let allowance = allowance_days(pool, user_id, year).await?;
    let (vacation_taken, vacation_pending, sick_taken) = taken_days(pool, user_id, year).await?;
    Ok(Summary {
        year,
        allowance,
        vacation_taken,
        vacation_pending,
        sick_taken,
        remaining: f64::from(allowance) - vacation_taken - vacation_pending,
    })
}

pub async fn me(user: AuthUser, Query(q): Query<YearQuery>) -> Result<Json<Value>, ApiError> {
    let year = q.year.unwrap_or_else(current_year);
    let pool = db::pool().await?;

    let public: PublicUser = sqlx::query_as(
        "SELECT id, email, name, role, active, must_change_password FROM users WHERE id = $1",
    )
    .bind(user.id)
    .fetch_one(pool)
    .await?;
    let summary = summary_for(pool, user.id, year).await?;

    Ok(Json(json!({ "user": public, "summary": summary })))
}

pub async fn overview(_user: AuthUser, Query(q): Query<YearQuery>) -> Result<Json<Value>, ApiError> {
    let year = q.year.unwrap_or_else(current_year);
    let pool = db::pool().await?;

    let users: Vec<PublicUser> = sqlx::query_as(
        "SELECT id, email, name, role, active, must_change_password
         FROM users WHERE active ORDER BY name",
    )
    .fetch_all(pool)
    .await?;

    let absences: Vec<Absence> = sqlx::query_as(
        "SELECT a.id, a.user_id, a.kind, a.start_date, a.end_date, a.business_days,
                a.status, a.day_part, a.note, a.decision_reason
         FROM absences a
         JOIN users u ON u.id = a.user_id
         WHERE u.active AND date_part('year', a.start_date) = $1
         ORDER BY a.start_date",
    )
    .bind(f64::from(year))
    .fetch_all(pool)
    .await?;

    let allowances: Vec<(Uuid, i32)> =
        sqlx::query_as("SELECT user_id, days FROM allowances WHERE year = $1")
            .bind(year)
            .fetch_all(pool)
            .await?;

    let user_summaries: Vec<Value> = users
        .iter()
        .map(|u| {
            let allowance = allowances
                .iter()
                .find(|(id, _)| *id == u.id)
                .map(|(_, days)| *days)
                .unwrap_or(0);
            let vacation_taken: f64 = absences
                .iter()
                .filter(|a| {
                    a.user_id == u.id
                        && a.kind == "vacation"
                        && matches!(a.status.as_str(), "approved" | "cancel_pending")
                })
                .map(|a| a.business_days)
                .sum();
            let vacation_pending: f64 = absences
                .iter()
                .filter(|a| a.user_id == u.id && a.kind == "vacation" && a.status == "pending")
                .map(|a| a.business_days)
                .sum();
            let sick_taken: f64 = absences
                .iter()
                .filter(|a| a.user_id == u.id && a.kind == "sick" && a.status != "denied")
                .map(|a| a.business_days)
                .sum();
            json!({
                "id": u.id,
                "name": u.name,
                "email": u.email,
                "role": u.role,
                "active": u.active,
                "allowance": allowance,
                "vacation_taken": vacation_taken,
                "vacation_pending": vacation_pending,
                "sick_taken": sick_taken,
                "remaining": f64::from(allowance) - vacation_taken - vacation_pending,
            })
        })
        .collect();

    Ok(Json(json!({ "users": user_summaries, "absences": absences })))
}
