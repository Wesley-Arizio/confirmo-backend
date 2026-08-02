use chrono::{DateTime, Utc};
use sqlx::types::Uuid;

use crate::database::firm::FirmDAO;

#[derive(Debug, PartialEq, Eq)]
pub struct Firm {
    pub id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
}

impl From<FirmDAO> for Firm {
    fn from(value: FirmDAO) -> Self {
        Self {
            id: value.id,
            name: value.name,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}
