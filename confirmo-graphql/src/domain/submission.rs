use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::database::submission::{SubmissionDAO, SubmissionStatusDAO};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmissionStatus {
    Pending,
    Approved,
    Rejected,
}

impl From<SubmissionStatus> for SubmissionStatusDAO {
    fn from(value: SubmissionStatus) -> Self {
        match value {
            SubmissionStatus::Pending => SubmissionStatusDAO::Pending,
            SubmissionStatus::Approved => SubmissionStatusDAO::Approved,
            SubmissionStatus::Rejected => SubmissionStatusDAO::Rejected,
        }
    }
}

impl From<SubmissionStatusDAO> for SubmissionStatus {
    fn from(value: SubmissionStatusDAO) -> Self {
        match value {
            SubmissionStatusDAO::Pending => SubmissionStatus::Pending,
            SubmissionStatusDAO::Approved => SubmissionStatus::Approved,
            SubmissionStatusDAO::Rejected => SubmissionStatus::Rejected,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Submission {
    pub id: Uuid,
    pub user_id: Uuid,
    pub status: SubmissionStatus,
    pub reviewed_by: Option<Uuid>,
    pub reviewed_at: Option<DateTime<Utc>>,
    pub reason: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<SubmissionDAO> for Submission {
    fn from(value: SubmissionDAO) -> Self {
        Self {
            id: value.id,
            user_id: value.user_id,
            status: value.status.into(),
            reviewed_by: value.reviewed_by,
            reviewed_at: value.reviewed_at,
            reason: value.reason,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

impl From<Submission> for SubmissionDAO {
    fn from(value: Submission) -> Self {
        Self {
            id: value.id,
            user_id: value.user_id,
            status: value.status.into(),
            reviewed_by: value.reviewed_by,
            reviewed_at: value.reviewed_at,
            reason: value.reason,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

pub struct CreateSubmissionInput {
    pub user_id: Uuid,
    pub image_key: String,
}
