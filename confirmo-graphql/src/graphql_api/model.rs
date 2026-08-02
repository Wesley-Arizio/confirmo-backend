use chrono::Utc;
use juniper::integrations::chrono::DateTime;
use juniper::{GraphQLEnum, GraphQLObject};

use crate::domain::lawyer::{Lawyer, Role, Status};

#[derive(GraphQLEnum)]
pub enum GqlStatus {
    PendingEmailVerification,
    EmailVerified,
    Active,
    Suspended,
    Disabled,
}

impl From<Status> for GqlStatus {
    fn from(value: Status) -> Self {
        match value {
            Status::PendingEmailVerification => GqlStatus::PendingEmailVerification,
            Status::EmailVerified => GqlStatus::EmailVerified,
            Status::Active => GqlStatus::Active,
            Status::Suspended => GqlStatus::Suspended,
            Status::Disabled => GqlStatus::Disabled,
        }
    }
}

impl From<GqlStatus> for Status {
    fn from(value: GqlStatus) -> Self {
        match value {
            GqlStatus::PendingEmailVerification => Status::PendingEmailVerification,
            GqlStatus::EmailVerified => Status::EmailVerified,
            GqlStatus::Active => Status::Active,
            GqlStatus::Suspended => Status::Suspended,
            GqlStatus::Disabled => Status::Disabled,
        }
    }
}

#[derive(GraphQLEnum)]
pub enum GqlRole {
    Lawyer,
    Client,
    Admin,
}

impl From<Role> for GqlRole {
    fn from(value: Role) -> Self {
        match value {
            Role::Lawyer => GqlRole::Lawyer,
            Role::Client => GqlRole::Client,
            Role::Admin => GqlRole::Admin,
        }
    }
}

impl From<GqlRole> for Role {
    fn from(value: GqlRole) -> Self {
        match value {
            GqlRole::Lawyer => Role::Lawyer,
            GqlRole::Client => Role::Client,
            GqlRole::Admin => Role::Admin,
        }
    }
}

#[derive(GraphQLObject)]
pub struct GqlLawyer {
    pub id: String,
    pub name: String,
    pub email: String,
    pub status: GqlStatus,
    pub oab_number: String,
    pub firm_id: String,
    pub email_verified_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
}

impl From<Lawyer> for GqlLawyer {
    fn from(value: Lawyer) -> Self {
        Self {
            id: value.id.to_string(),
            name: value.name,
            email: value.email,
            status: value.status.into(),
            oab_number: value.oab_number,
            firm_id: value.firm_id.to_string(),
            email_verified_at: value.email_verified_at,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}
