use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use tokio::sync::OnceCell;

use crate::auth;
use crate::dates::current_year;
use crate::error::ApiError;

static POOL: OnceCell<PgPool> = OnceCell::const_new();

pub async fn pool() -> Result<&'static PgPool, ApiError> {
    POOL.get_or_try_init(init).await
}

async fn init() -> Result<PgPool, ApiError> {
    let url = std::env::var("DATABASE_URL")
        .map_err(|_| ApiError::Internal("DATABASE_URL not set".to_string()))?;
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&url)
        .await?;
    bootstrap_admin(&pool).await?;
    Ok(pool)
}

const DEFAULT_ALLOWANCE_DAYS: i32 = 30;

/// If the users table is empty, create the initial admin from
/// ADMIN_EMAIL / ADMIN_PASSWORD env vars (idempotent; runs once per instance).
async fn bootstrap_admin(pool: &PgPool) -> Result<(), ApiError> {
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM users")
        .fetch_one(pool)
        .await?;
    if count > 0 {
        return Ok(());
    }

    let (Ok(email), Ok(password)) = (std::env::var("ADMIN_EMAIL"), std::env::var("ADMIN_PASSWORD"))
    else {
        eprintln!("users table empty and ADMIN_EMAIL/ADMIN_PASSWORD not set; skipping bootstrap");
        return Ok(());
    };

    let hash = auth::hash_password(&password)?;
    let user_id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO users (email, name, password_hash, role)
         VALUES (lower($1), $2, $3, 'admin')
         ON CONFLICT DO NOTHING
         RETURNING id",
    )
    .bind(&email)
    .bind("Scott")
    .bind(&hash)
    .fetch_one(pool)
    .await?;

    sqlx::query(
        "INSERT INTO allowances (user_id, year, days) VALUES ($1, $2, $3)
         ON CONFLICT (user_id, year) DO NOTHING",
    )
    .bind(user_id)
    .bind(current_year())
    .bind(DEFAULT_ALLOWANCE_DAYS)
    .execute(pool)
    .await?;

    println!("bootstrapped admin user {email}");
    Ok(())
}
