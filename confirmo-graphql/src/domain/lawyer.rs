use chrono::{DateTime, Utc};
use sqlx::types::Uuid;

use crate::database::lawyer::{LawyerDAO, RoleDAO, StatusDAO, UserDAO};

#[derive(Debug, PartialEq, Eq)]
pub enum Status {
    PendingEmailVerification,
    EmailVerified,
    Active,
    Suspended,
    Disabled,
}

impl From<StatusDAO> for Status {
    fn from(value: StatusDAO) -> Self {
        match value {
            StatusDAO::PendingEmailVerification => Status::PendingEmailVerification,
            StatusDAO::EmailVerified => Status::EmailVerified,
            StatusDAO::Active => Status::Active,
            StatusDAO::Suspended => Status::Suspended,
            StatusDAO::Disabled => Status::Disabled,
        }
    }
}

impl From<Status> for StatusDAO {
    fn from(value: Status) -> Self {
        match value {
            Status::PendingEmailVerification => StatusDAO::PendingEmailVerification,
            Status::EmailVerified => StatusDAO::EmailVerified,
            Status::Active => StatusDAO::Active,
            Status::Suspended => StatusDAO::Suspended,
            Status::Disabled => StatusDAO::Disabled,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Role {
    Lawyer,
    Client,
    Admin,
}

impl From<RoleDAO> for Role {
    fn from(value: RoleDAO) -> Self {
        match value {
            RoleDAO::Lawyer => Role::Lawyer,
            RoleDAO::Client => Role::Client,
            RoleDAO::Admin => Role::Admin,
        }
    }
}

impl From<Role> for RoleDAO {
    fn from(value: Role) -> Self {
        match value {
            Role::Lawyer => RoleDAO::Lawyer,
            Role::Client => RoleDAO::Client,
            Role::Admin => RoleDAO::Admin,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Lawyer {
    pub id: Uuid,
    pub name: String,
    pub email: String,
    pub status: Status,
    pub role: Role,
    pub email_verified_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
    pub oab_number: String,
}

pub struct CreateLawyerInput {
    pub user_id: Uuid,
    pub name: String,
    pub email: String,
    pub status: Status,
    pub role: Role,
    pub oab_number: String,
}

impl Lawyer {
    pub fn from_dao(user: UserDAO, lawyer: LawyerDAO) -> Lawyer {
        Self {
            id: user.id,
            name: user.name,
            email: user.email,
            status: user.status.into(),
            role: user.role.into(),
            email_verified_at: user.email_verified_at,
            created_at: user.created_at,
            oab_number: lawyer.oab_number,
            updated_at: user.updated_at,
        }
    }
}
