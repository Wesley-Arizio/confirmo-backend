use std::sync::Arc;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use sqlx::{PgPool, Type};

use crate::{
    database::{error::DatabaseError, traits::SubmissionRepository},
    domain::submission::{CreateSubmissionInput, Submission},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Type)]
#[sqlx(type_name = "submission_status", rename_all = "snake_case")]
pub enum SubmissionStatusDAO {
    Pending,
    Approved,
    Rejected,
}

#[derive(sqlx::FromRow, Debug, PartialEq, Eq)]
pub struct SubmissionDAO {
    pub id: Uuid,
    pub user_id: Uuid,
    pub status: SubmissionStatusDAO,
    pub reviewed_by: Option<Uuid>,
    pub reviewed_at: Option<DateTime<Utc>>,
    pub reason: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct SubmissionPostgresRepository {
    pub pg_pool: Arc<PgPool>,
}

impl SubmissionPostgresRepository {
    pub fn new(pg_pool: Arc<PgPool>) -> Self {
        Self { pg_pool }
    }
}

#[async_trait::async_trait]
impl SubmissionRepository for SubmissionPostgresRepository {
    async fn create_submission(
        &self,
        input: CreateSubmissionInput,
    ) -> Result<Submission, DatabaseError> {
        let submission: SubmissionDAO = sqlx::query_as(
            r#"
            INSERT INTO submissions 
                (user_id, image_key)
            VALUES
                ($1, $2) 
            RETURNING 
                id,
                user_id,
                status,
                image_key,
                reviewed_by,
                reviewed_at,
                reason,
                created_at,
                updated_at;
            "#,
        )
        .bind(input.user_id)
        .bind(input.image_key)
        .fetch_one(&*self.pg_pool)
        .await?;

        Ok(submission.into())
    }
}
