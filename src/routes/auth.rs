use axum::Json;
use axum::http::StatusCode;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::auth::{self, AuthUser};
use crate::db;
use crate::error::ApiError;
use crate::models::UserAuthRow;

#[derive(Deserialize)]
pub struct LoginBody {
    pub email: String,
    pub password: String,
}

pub async fn login(Json(body): Json<LoginBody>) -> Result<Json<Value>, ApiError> {
    let pool = db::pool().await?;
    let user: Option<UserAuthRow> = sqlx::query_as(
        "SELECT id, email, name, role, active, must_change_password, password_hash
         FROM users WHERE lower(email) = lower($1)",
    )
    .bind(body.email.trim())
    .fetch_optional(pool)
    .await?;

    let Some(user) = user else {
        return Err(ApiError::Unauthorized);
    };
    if !user.active || !auth::verify_password(&body.password, &user.password_hash) {
        return Err(ApiError::Unauthorized);
    }

    let token = auth::issue_token(user.id)?;
    Ok(Json(json!({ "token": token, "user": user.public() })))
}

#[derive(Deserialize)]
pub struct UpdateMeBody {
    pub name: String,
}

pub async fn update_me(
    user: AuthUser,
    Json(body): Json<UpdateMeBody>,
) -> Result<Json<crate::models::PublicUser>, ApiError> {
    let name = body.name.trim();
    if name.is_empty() || name.len() > 80 {
        return Err(ApiError::Unprocessable("name must be 1-80 characters".to_string()));
    }
    let pool = db::pool().await?;
    let updated: crate::models::PublicUser = sqlx::query_as(
        "UPDATE users SET name = $2 WHERE id = $1
         RETURNING id, email, name, role, active, must_change_password",
    )
    .bind(user.id)
    .bind(name)
    .fetch_one(pool)
    .await?;
    Ok(Json(updated))
}

#[derive(Deserialize)]
pub struct ChangePasswordBody {
    pub current_password: String,
    pub new_password: String,
}

pub async fn change_password(
    user: AuthUser,
    Json(body): Json<ChangePasswordBody>,
) -> Result<StatusCode, ApiError> {
    if body.new_password.len() < 8 {
        return Err(ApiError::Unprocessable(
            "new password must be at least 8 characters".to_string(),
        ));
    }

    let pool = db::pool().await?;
    let current_hash: String = sqlx::query_scalar("SELECT password_hash FROM users WHERE id = $1")
        .bind(user.id)
        .fetch_one(pool)
        .await?;
    if !auth::verify_password(&body.current_password, &current_hash) {
        return Err(ApiError::Unprocessable("current password is incorrect".to_string()));
    }

    let new_hash = auth::hash_password(&body.new_password)?;
    sqlx::query(
        "UPDATE users SET password_hash = $1, must_change_password = false WHERE id = $2",
    )
    .bind(&new_hash)
    .bind(user.id)
    .execute(pool)
    .await?;

    Ok(StatusCode::NO_CONTENT)
}
