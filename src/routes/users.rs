use axum::Json;
use axum::extract::Path;
use axum::http::StatusCode;
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::auth::{self, AuthUser};
use crate::dates::current_year;
use crate::db;
use crate::error::ApiError;
use crate::models::PublicUser;

pub async fn list(user: AuthUser) -> Result<Json<Vec<Value>>, ApiError> {
    user.require_admin()?;
    let pool = db::pool().await?;

    let users: Vec<PublicUser> = sqlx::query_as(
        "SELECT id, email, name, role, active, must_change_password
         FROM users ORDER BY created_at",
    )
    .fetch_all(pool)
    .await?;

    let allowances: Vec<(Uuid, i32, i32)> =
        sqlx::query_as("SELECT user_id, year, days FROM allowances ORDER BY year")
            .fetch_all(pool)
            .await?;

    let result = users
        .iter()
        .map(|u| {
            let user_allowances: Vec<Value> = allowances
                .iter()
                .filter(|(id, _, _)| *id == u.id)
                .map(|(_, year, days)| json!({ "year": year, "days": days }))
                .collect();
            json!({
                "id": u.id,
                "email": u.email,
                "name": u.name,
                "role": u.role,
                "active": u.active,
                "allowances": user_allowances,
            })
        })
        .collect();

    Ok(Json(result))
}

fn validate_role(role: &str) -> Result<(), ApiError> {
    if role == "admin" || role == "member" {
        Ok(())
    } else {
        Err(ApiError::Unprocessable("role must be 'admin' or 'member'".to_string()))
    }
}

fn validate_password(password: &str) -> Result<(), ApiError> {
    if password.len() < 8 {
        return Err(ApiError::Unprocessable(
            "password must be at least 8 characters".to_string(),
        ));
    }
    Ok(())
}

#[derive(Deserialize)]
pub struct CreateUserBody {
    pub email: String,
    pub name: String,
    pub password: String,
    pub role: String,
    pub allowance_days: i32,
}

pub async fn create(
    user: AuthUser,
    Json(body): Json<CreateUserBody>,
) -> Result<(StatusCode, Json<PublicUser>), ApiError> {
    user.require_admin()?;
    let email = body.email.trim().to_lowercase();
    if !email.contains('@') {
        return Err(ApiError::Unprocessable("invalid email".to_string()));
    }
    validate_role(&body.role)?;
    validate_password(&body.password)?;
    if body.allowance_days < 0 {
        return Err(ApiError::Unprocessable("allowance must be >= 0".to_string()));
    }

    let pool = db::pool().await?;
    let hash = auth::hash_password(&body.password)?;

    let created: PublicUser = sqlx::query_as(
        "INSERT INTO users (email, name, password_hash, role, must_change_password)
         VALUES ($1, $2, $3, $4, true)
         RETURNING id, email, name, role, active, must_change_password",
    )
    .bind(&email)
    .bind(body.name.trim())
    .bind(&hash)
    .bind(&body.role)
    .fetch_one(pool)
    .await
    .map_err(|e| match ApiError::from(e) {
        ApiError::Conflict(_) => ApiError::Conflict("a user with this email already exists".to_string()),
        other => other,
    })?;

    sqlx::query(
        "INSERT INTO allowances (user_id, year, days) VALUES ($1, $2, $3)
         ON CONFLICT (user_id, year) DO UPDATE SET days = EXCLUDED.days",
    )
    .bind(created.id)
    .bind(current_year())
    .bind(body.allowance_days)
    .execute(pool)
    .await?;

    Ok((StatusCode::CREATED, Json(created)))
}

#[derive(Deserialize)]
pub struct PatchUserBody {
    pub name: Option<String>,
    pub role: Option<String>,
    pub active: Option<bool>,
    pub password: Option<String>,
}

pub async fn update(
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<PatchUserBody>,
) -> Result<Json<PublicUser>, ApiError> {
    user.require_admin()?;
    if let Some(role) = &body.role {
        validate_role(role)?;
    }
    let password_hash = match &body.password {
        Some(p) => {
            validate_password(p)?;
            Some(auth::hash_password(p)?)
        }
        None => None,
    };

    let pool = db::pool().await?;
    let updated: Option<PublicUser> = sqlx::query_as(
        "UPDATE users SET
            name = COALESCE($2, name),
            role = COALESCE($3, role),
            active = COALESCE($4, active),
            password_hash = COALESCE($5, password_hash),
            must_change_password = CASE WHEN $5 IS NOT NULL THEN true ELSE must_change_password END
         WHERE id = $1
         RETURNING id, email, name, role, active, must_change_password",
    )
    .bind(id)
    .bind(body.name.as_deref().map(str::trim))
    .bind(body.role.as_deref())
    .bind(body.active)
    .bind(password_hash.as_deref())
    .fetch_optional(pool)
    .await?;

    updated.map(Json).ok_or(ApiError::NotFound)
}

#[derive(Deserialize)]
pub struct AllowanceBody {
    pub days: i32,
}

pub async fn put_allowance(
    user: AuthUser,
    Path((id, year)): Path<(Uuid, i32)>,
    Json(body): Json<AllowanceBody>,
) -> Result<Json<Value>, ApiError> {
    user.require_admin()?;
    if body.days < 0 {
        return Err(ApiError::Unprocessable("allowance must be >= 0".to_string()));
    }
    if !(2000..=2100).contains(&year) {
        return Err(ApiError::Unprocessable("year out of range".to_string()));
    }

    let pool = db::pool().await?;
    let exists: Option<Uuid> = sqlx::query_scalar("SELECT id FROM users WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    if exists.is_none() {
        return Err(ApiError::NotFound);
    }

    sqlx::query(
        "INSERT INTO allowances (user_id, year, days) VALUES ($1, $2, $3)
         ON CONFLICT (user_id, year) DO UPDATE SET days = EXCLUDED.days",
    )
    .bind(id)
    .bind(year)
    .bind(body.days)
    .execute(pool)
    .await?;

    Ok(Json(json!({ "user_id": id, "year": year, "days": body.days })))
}
