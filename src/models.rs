use chrono::NaiveDate;
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Serialize, FromRow, Clone)]
pub struct PublicUser {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub role: String,
    pub active: bool,
    pub must_change_password: bool,
}

#[derive(FromRow)]
pub struct UserAuthRow {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub role: String,
    pub active: bool,
    pub must_change_password: bool,
    pub password_hash: String,
}

impl UserAuthRow {
    pub fn public(&self) -> PublicUser {
        PublicUser {
            id: self.id,
            email: self.email.clone(),
            name: self.name.clone(),
            role: self.role.clone(),
            active: self.active,
            must_change_password: self.must_change_password,
        }
    }
}

#[derive(Serialize, FromRow)]
pub struct Absence {
    pub id: Uuid,
    pub user_id: Uuid,
    pub kind: String,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub business_days: f64,
    pub status: String,
    pub day_part: String,
    pub note: Option<String>,
    pub decision_reason: Option<String>,
}

pub const ABSENCE_COLUMNS: &str =
    "id, user_id, kind, start_date, end_date, business_days, status, day_part, note, decision_reason";

#[derive(Serialize)]
pub struct Summary {
    pub year: i32,
    pub allowance: i32,
    pub vacation_taken: f64,
    pub vacation_pending: f64,
    pub sick_taken: f64,
    pub remaining: f64,
}

#[derive(Serialize, FromRow)]
pub struct CompanyDay {
    pub id: Uuid,
    pub name: String,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
}

#[derive(Serialize, FromRow)]
pub struct AllowanceRow {
    pub year: i32,
    pub days: i32,
}
