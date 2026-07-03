use axum::http::{HeaderValue, Method, header};
use axum::routing::{delete, get, patch, post, put};
use axum::{Json, Router};
use serde_json::json;
use tower_http::cors::{AllowOrigin, CorsLayer};

pub mod auth;
pub mod dates;
pub mod db;
pub mod error;
pub mod models;
pub mod routes;

async fn health() -> Json<serde_json::Value> {
    Json(json!({ "ok": true }))
}

fn cors() -> CorsLayer {
    // Tauri webview origins differ by OS; localhost:1420 is the Vite dev server.
    let mut origins: Vec<String> = vec![
        "tauri://localhost".to_string(),
        "http://tauri.localhost".to_string(),
        "http://localhost:1420".to_string(),
    ];
    if let Ok(extra) = std::env::var("ALLOWED_ORIGINS") {
        origins.extend(extra.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()));
    }
    let origins: Vec<HeaderValue> = origins
        .iter()
        .filter_map(|o| o.parse::<HeaderValue>().ok())
        .collect();

    CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PATCH,
            Method::PUT,
            Method::DELETE,
        ])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE])
}

pub fn app() -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route(
            "/api/settings",
            get(routes::settings::get).put(routes::settings::update),
        )
        .route("/api/auth/login", post(routes::auth::login))
        .route("/api/me", get(routes::overview::me))
        .route("/api/me/password", post(routes::auth::change_password))
        .route(
            "/api/absences",
            get(routes::absences::list).post(routes::absences::create),
        )
        .route("/api/absences/{id}", delete(routes::absences::remove))
        .route("/api/absences/{id}/approve", post(routes::absences::approve))
        .route("/api/absences/{id}/deny", post(routes::absences::deny))
        .route(
            "/api/company-days",
            get(routes::company_days::list).post(routes::company_days::create),
        )
        .route(
            "/api/company-days/{id}",
            delete(routes::company_days::remove),
        )
        .route("/api/overview", get(routes::overview::overview))
        .route(
            "/api/users",
            get(routes::users::list).post(routes::users::create),
        )
        .route("/api/users/{id}", patch(routes::users::update))
        .route(
            "/api/users/{id}/allowances/{year}",
            put(routes::users::put_allowance),
        )
        .layer(cors())
}
