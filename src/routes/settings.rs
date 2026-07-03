use axum::Json;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::auth::AuthUser;
use crate::db;
use crate::error::ApiError;

const MAX_LOGO_BYTES: usize = 300 * 1024; // data-URL text length cap

/// Public: the login screen needs branding before authentication. When
/// `hide_login_branding` is on, anonymous requests get colors only; the
/// company name and logo are withheld until after login.
pub async fn get(user: Option<AuthUser>) -> Result<Json<Value>, ApiError> {
    let pool = db::pool().await?;
    let (company_name, accent_color, accent_color2, logo_data, require_approval, hide_login_branding): (
        String,
        String,
        Option<String>,
        Option<String>,
        bool,
        bool,
    ) = sqlx::query_as(
        "SELECT company_name, accent_color, accent_color2, logo_data, require_approval, hide_login_branding
         FROM settings WHERE id",
    )
    .fetch_one(pool)
    .await?;

    let mask = user.is_none() && hide_login_branding;
    Ok(Json(json!({
        "company_name": if mask { "".to_string() } else { company_name },
        "accent_color": accent_color,
        "accent_color2": accent_color2,
        "logo_data": if mask { None } else { logo_data },
        "require_approval": require_approval,
        "hide_login_branding": hide_login_branding,
    })))
}

#[derive(Deserialize)]
pub struct UpdateBody {
    pub company_name: String,
    pub accent_color: String,
    #[serde(default)]
    pub accent_color2: Option<String>,
    #[serde(default)]
    pub logo_data: Option<String>,
    #[serde(default)]
    pub require_approval: bool,
    #[serde(default)]
    pub hide_login_branding: bool,
}

fn valid_hex_color(s: &str) -> bool {
    s.len() == 7 && s.starts_with('#') && s[1..].chars().all(|c| c.is_ascii_hexdigit())
}

pub async fn update(user: AuthUser, Json(body): Json<UpdateBody>) -> Result<Json<Value>, ApiError> {
    user.require_admin()?;
    let name = body.company_name.trim();
    if name.is_empty() || name.len() > 60 {
        return Err(ApiError::Unprocessable(
            "company name must be 1-60 characters".to_string(),
        ));
    }
    if !valid_hex_color(&body.accent_color) {
        return Err(ApiError::Unprocessable(
            "accent color must be a hex color like #0d9488".to_string(),
        ));
    }
    if let Some(c2) = &body.accent_color2
        && !valid_hex_color(c2)
    {
        return Err(ApiError::Unprocessable(
            "gradient color must be a hex color like #7c3aed".to_string(),
        ));
    }
    if let Some(logo) = &body.logo_data {
        if !logo.starts_with("data:image/") {
            return Err(ApiError::Unprocessable("logo must be an image".to_string()));
        }
        if logo.len() > MAX_LOGO_BYTES {
            return Err(ApiError::Unprocessable(
                "logo is too large (max ~200KB image)".to_string(),
            ));
        }
    }

    let accent = body.accent_color.to_lowercase();
    let accent2 = body.accent_color2.as_ref().map(|c| c.to_lowercase());

    let pool = db::pool().await?;
    sqlx::query(
        "UPDATE settings SET
            company_name = $1,
            accent_color = $2,
            accent_color2 = $3,
            logo_data = $4,
            require_approval = $5,
            hide_login_branding = $6,
            updated_at = now()
         WHERE id",
    )
    .bind(name)
    .bind(&accent)
    .bind(&accent2)
    .bind(&body.logo_data)
    .bind(body.require_approval)
    .bind(body.hide_login_branding)
    .execute(pool)
    .await?;

    Ok(Json(json!({
        "company_name": name,
        "accent_color": accent,
        "accent_color2": accent2,
        "logo_data": body.logo_data,
        "require_approval": body.require_approval,
        "hide_login_branding": body.hide_login_branding,
    })))
}
