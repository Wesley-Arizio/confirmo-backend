use chrono::{DateTime, Utc};
use sqlx::types::Uuid;

#[derive(sqlx::FromRow, Debug, PartialEq, Eq)]
pub struct FirmDAO {
    pub id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
}
