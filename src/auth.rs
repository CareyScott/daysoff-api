use argon2::Argon2;
use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db;
use crate::error::ApiError;

const TOKEN_TTL_SECS: i64 = 60 * 60 * 24 * 30; // 30 days

pub fn hash_password(password: &str) -> Result<String, ApiError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| ApiError::Internal(format!("hash error: {e}")))
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

#[derive(Serialize, Deserialize)]
struct Claims {
    sub: Uuid,
    exp: i64,
}

fn secret() -> Result<Vec<u8>, ApiError> {
    std::env::var("JWT_SECRET")
        .map(|s| s.into_bytes())
        .map_err(|_| ApiError::Internal("JWT_SECRET not set".to_string()))
}

pub fn issue_token(user_id: Uuid) -> Result<String, ApiError> {
    let claims = Claims {
        sub: user_id,
        exp: chrono::Utc::now().timestamp() + TOKEN_TTL_SECS,
    };
    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(&secret()?),
    )
    .map_err(|e| ApiError::Internal(format!("jwt encode error: {e}")))
}

fn verify_token(token: &str) -> Result<Uuid, ApiError> {
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(&secret()?),
        &Validation::new(Algorithm::HS256),
    )
    .map_err(|_| ApiError::Unauthorized)?;
    Ok(data.claims.sub)
}

/// Authenticated user extractor. Validates the bearer token, then loads
/// role + active from the DB so deactivation and role changes apply
/// immediately (no token revocation needed).
pub struct AuthUser {
    pub id: Uuid,
    pub role: String,
}

impl AuthUser {
    pub fn is_admin(&self) -> bool {
        self.role == "admin"
    }

    pub fn require_admin(&self) -> Result<(), ApiError> {
        if self.is_admin() { Ok(()) } else { Err(ApiError::Forbidden) }
    }
}

impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or(ApiError::Unauthorized)?;
        let token = header.strip_prefix("Bearer ").ok_or(ApiError::Unauthorized)?;
        let user_id = verify_token(token)?;

        let pool = db::pool().await?;
        let row: Option<(String, bool)> =
            sqlx::query_as("SELECT role, active FROM users WHERE id = $1")
                .bind(user_id)
                .fetch_optional(pool)
                .await?;
        match row {
            Some((role, true)) => Ok(AuthUser { id: user_id, role }),
            _ => Err(ApiError::Unauthorized),
        }
    }
}
