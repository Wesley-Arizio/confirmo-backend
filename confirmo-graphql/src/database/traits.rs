use uuid::Uuid;

use crate::database::error::DatabaseError;
use crate::domain::{
    lawyer::{CreateLawyerInput, Lawyer},
    submission::{CreateSubmissionInput, Submission},
};

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
    async fn verify_face(&self, user_id: Uuid) -> Result<bool, DatabaseError>;
}

#[mockall::automock]
#[async_trait::async_trait]
pub trait SubmissionRepository: Send + Sync + 'static {
    async fn create_submission(
        &self,
        input: CreateSubmissionInput,
    ) -> Result<Submission, DatabaseError>;
}
