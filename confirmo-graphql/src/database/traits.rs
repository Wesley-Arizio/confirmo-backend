use uuid::Uuid;

use crate::database::error::DatabaseError;
use crate::domain::lawyer::{CreateLawyerInput, Lawyer};

#[derive(Debug)]
pub enum UserBy {
    Id(Uuid),
    Email(String),
}

#[mockall::automock]
#[async_trait::async_trait]
pub trait LawyerRepository: Send + Sync + 'static {
    async fn create_lawyer(&self, input: CreateLawyerInput) -> Result<Lawyer, DatabaseError>;
    async fn get_lawyer(&self, by: UserBy) -> Result<Option<Lawyer>, DatabaseError>;
    async fn exists_by_oab_number(&self, oab_number: &str) -> Result<bool, DatabaseError>;
    async fn verify_email(&self, user_id: Uuid) -> Result<bool, DatabaseError>;
}
