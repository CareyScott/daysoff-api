use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

#[derive(Debug)]
pub enum ApiError {
    Unauthorized,
    Forbidden,
    NotFound,
    Conflict(String),
    Unprocessable(String),
    Internal(String),
}

impl ApiError {
    fn status(&self) -> StatusCode {
        match self {
            ApiError::Unauthorized => StatusCode::UNAUTHORIZED,
            ApiError::Forbidden => StatusCode::FORBIDDEN,
            ApiError::NotFound => StatusCode::NOT_FOUND,
            ApiError::Conflict(_) => StatusCode::CONFLICT,
            ApiError::Unprocessable(_) => StatusCode::UNPROCESSABLE_ENTITY,
            ApiError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn message(&self) -> String {
        match self {
            ApiError::Unauthorized => "unauthorized".to_string(),
            ApiError::Forbidden => "forbidden".to_string(),
            ApiError::NotFound => "not found".to_string(),
            ApiError::Conflict(m) | ApiError::Unprocessable(m) => m.clone(),
            // Don't leak internals to clients; log them instead.
            ApiError::Internal(_) => "internal server error".to_string(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        if let ApiError::Internal(detail) = &self {
            eprintln!("internal error: {detail}");
        }
        (self.status(), Json(json!({ "error": self.message() }))).into_response()
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(err: sqlx::Error) -> Self {
        if let sqlx::Error::Database(db_err) = &err {
            match db_err.code().as_deref() {
                // exclusion constraint (overlapping absence)
                Some("23P01") => {
                    return ApiError::Conflict("overlaps an existing absence".to_string());
                }
                // unique violation (duplicate email)
                Some("23505") => return ApiError::Conflict("already exists".to_string()),
                _ => {}
            }
        }
        ApiError::Internal(format!("database error: {err}"))
    }
}
