use std::{sync::Arc, time::Duration};

use confirmo_shared::constants::EMAIL_VERIFICATION_CODE_LENGTH;
use confirmo_utils::{email::validate_email, password::validate_password};
use thiserror::Error;
use uuid::Uuid;

use crate::{
    application::ports::{
        AuthApi, AuthApiError, MessageProducer, PresignerError, ProducerError, S3Presigner,
    },
    database::{
        error::DatabaseError,
        traits::{LawyerRepository, SubmissionRepository, UserBy},
    },
    domain::{
        lawyer::{CreateLawyerInput, Lawyer, Role, Status},
        submission::CreateSubmissionInput,
    },
};

#[derive(Debug, Error)]
pub enum CoreServiceError {
    #[error("User not found")]
    UserNotFound,

    #[error("Internal error")]
    Internal,

    #[error("Uer email not verified")]
    UserEmailNotVerified,

    #[error("Invalid Status Transition from: '{from:?}', to: '{to:?}'")]
    InvalidStatusTransition { from: Status, to: Status },

    #[error("S3 presign error")]
    Presign(#[source] Box<PresignerError>),

    #[error("Message producer error")]
    Producer(#[source] Box<ProducerError>),

    #[error("Invalid email format")]
    InvalidEmailFormat,

    #[error("Invalid image key format")]
    InvalidImageKeyFormat,

    #[error("Lawyer with this OAB Number already exists")]
    LawyerWithOABNumberAlreadyExists,

    #[error("Invalid password format: '{0}'")]
    InvalidPasswordFormat(String),

    #[error("Invalid verification format")]
    InvalidVerificationCodeFormat,

    #[error("Invalid argument: '{0}'")]
    InvalidArgument(String),

    #[error("Email already exists")]
    EmailAlreadyExists,

    #[error("Auth service unavailable")]
    AuthServiceUnavailable,

    #[error("Invalid credentials")]
    InvalidCredentials,

    #[error("Auth service error")]
    Auth(#[source] AuthApiError),
}

impl From<PresignerError> for CoreServiceError {
    fn from(value: PresignerError) -> Self {
        CoreServiceError::Presign(Box::new(value))
    }
}

impl From<ProducerError> for CoreServiceError {
    fn from(value: ProducerError) -> Self {
        CoreServiceError::Producer(Box::new(value))
    }
}

impl From<DatabaseError> for CoreServiceError {
    fn from(value: DatabaseError) -> Self {
        tracing::error!("Database error: {:#?}", value);
        CoreServiceError::Internal
    }
}

impl From<AuthApiError> for CoreServiceError {
    fn from(e: AuthApiError) -> Self {
        match e {
            AuthApiError::EmailAlreadyExists => CoreServiceError::EmailAlreadyExists,
            AuthApiError::Unavailable => CoreServiceError::AuthServiceUnavailable,
            AuthApiError::InvalidCredentials => CoreServiceError::InvalidCredentials,
            AuthApiError::InvalidArgument(message) => CoreServiceError::InvalidArgument(message),
            AuthApiError::ParseUserIdFailed(_) => CoreServiceError::Internal,
            AuthApiError::Grpc(_) => CoreServiceError::Auth(e),
        }
    }
}

pub struct CoreService {
    lawyer_repository: Arc<dyn LawyerRepository>,
    submission_repository: Arc<dyn SubmissionRepository>,
    s3_presigner: Arc<dyn S3Presigner>,
    grpc_auth_api: Arc<dyn AuthApi>,
    producer: Arc<dyn MessageProducer>,
}

#[derive(Debug)]
pub struct PresignedUploadResponse {
    pub key: String,
    pub upload_url: String,
}

impl CoreService {
    pub fn new(
        lawyer_repository: Arc<dyn LawyerRepository>,
        submission_repository: Arc<dyn SubmissionRepository>,
        s3_presigner: Arc<dyn S3Presigner>,
        producer: Arc<dyn MessageProducer>,
        grpc_auth_api: Arc<dyn AuthApi>,
    ) -> Self {
        Self {
            lawyer_repository,
            submission_repository,
            s3_presigner,
            grpc_auth_api,
            producer,
        }
    }
    pub async fn email_verified(&self, user_id: &str) -> Result<(), CoreServiceError> {
        tracing::info!("User {}", user_id);
        let user_id = Uuid::parse_str(user_id).map_err(|e| {
            tracing::error!("Error parsing user_id {:#?}", e);
            CoreServiceError::Internal
        })?;

        self.lawyer_repository
            .verify_email(user_id)
            .await
            .map_err(|e| {
                tracing::error!("Database Error: {:#?}", e);
                CoreServiceError::Internal
            })?;

        Ok(())
    }

    pub async fn get_presigned_url(
        &self,
        email: String,
    ) -> Result<PresignedUploadResponse, CoreServiceError> {
        let Some(user) = self
            .lawyer_repository
            .get_lawyer(UserBy::Email(email))
            .await?
        else {
            return Err(CoreServiceError::UserNotFound);
        };

        if user.status != Status::EmailVerified {
            return Err(CoreServiceError::UserEmailNotVerified);
        }

        let expires_in = Duration::from_mins(5);
        let key = format!("user_verification_{}", user.id);
        let upload_url = self
            .s3_presigner
            .presign_put_object(&key, "image/jpeg", expires_in)
            .await?;

        tracing::info!("Presigned url created successfuly");

        Ok(PresignedUploadResponse { key, upload_url })
    }

    pub async fn submit_profile_for_review(
        &self,
        email: String,
        image_key: String,
    ) -> Result<(), CoreServiceError> {
        if !validate_email(&email) {
            tracing::error!("invalid email format");
            return Err(CoreServiceError::InvalidEmailFormat);
        }

        if image_key.trim().is_empty() {
            tracing::warn!("empty image_key");
            return Err(CoreServiceError::InvalidImageKeyFormat);
        }

        let Some(user) = self
            .lawyer_repository
            .get_lawyer(UserBy::Email(email))
            .await?
        else {
            return Err(CoreServiceError::UserNotFound);
        };

        if !user.status.can_request_face_verification() {
            return Err(CoreServiceError::InvalidStatusTransition {
                from: user.status,
                to: Status::PendingFaceReview,
            });
        }

        self.lawyer_repository.verify_face(user.id).await?;
        self.submission_repository
            .create_submission(CreateSubmissionInput {
                user_id: user.id,
                image_key,
            })
            .await?;
        self.producer.profile_review_requested(user.id).await?;

        Ok(())
    }

    pub async fn profile_review_requested(&self, user_id: String) -> Result<(), CoreServiceError> {
        tracing::info!(
            "User submitted their profile for review, send email to admin: {:#?}",
            user_id
        );

        Ok(())
    }

    pub async fn lawyer(&self, email: String) -> Result<Lawyer, CoreServiceError> {
        if !validate_email(&email) {
            return Err(CoreServiceError::InvalidEmailFormat);
        }
        if let Some(lawyer) = self
            .lawyer_repository
            .get_lawyer(UserBy::Email(email))
            .await?
        {
            return Ok(lawyer);
        };

        Err(CoreServiceError::UserNotFound)
    }

    pub async fn create_lawyer_account(
        &self,
        email: String,
        password: String,
        oab_number: String,
        name: String,
    ) -> Result<Lawyer, CoreServiceError> {
        if !validate_email(&email) {
            return Err(CoreServiceError::InvalidEmailFormat);
        }
        validate_password(&password)
            .map_err(|e| CoreServiceError::InvalidPasswordFormat(e.to_string()))?;

        tracing::debug!("Validated email and password");

        let exists_by_oab_number = self
            .lawyer_repository
            .exists_by_oab_number(&oab_number)
            .await?;

        if exists_by_oab_number {
            tracing::error!("Lawyer with oab '{}' already exists", oab_number);
            return Err(CoreServiceError::LawyerWithOABNumberAlreadyExists);
        }

        let user_id = self
            .grpc_auth_api
            .create_credential(&email, &password)
            .await?;

        let create_lawyer_input = CreateLawyerInput {
            name,
            user_id,
            email: email.clone(),
            status: Status::PendingEmailVerification,
            role: Role::Lawyer,
            oab_number,
        };

        let lawyer = self
            .lawyer_repository
            .create_lawyer(create_lawyer_input)
            .await?;

        tracing::info!(
            lawyer_id = %lawyer.id,
            "Lawyer account created"
        );

        self.grpc_auth_api
            .request_email_verification(&email)
            .await?;

        Ok(lawyer)
    }

    pub async fn request_new_email_verifcation_code(
        &self,
        email: &str,
    ) -> Result<bool, CoreServiceError> {
        if !validate_email(email) {
            return Err(CoreServiceError::InvalidEmailFormat);
        }

        let Some(user) = self
            .lawyer_repository
            .get_lawyer(UserBy::Email(email.to_owned()))
            .await?
        else {
            return Err(CoreServiceError::InvalidCredentials);
        };

        self.grpc_auth_api.request_email_verification(email).await?;
        Ok(true)
    }

    pub async fn verify_email(&self, email: &str, code: &str) -> Result<bool, CoreServiceError> {
        if !validate_email(email) {
            return Err(CoreServiceError::InvalidEmailFormat);
        }

        if code.len() < EMAIL_VERIFICATION_CODE_LENGTH {
            tracing::warn!("Invalid Code Format");
            return Err(CoreServiceError::InvalidVerificationCodeFormat);
        }

        self.grpc_auth_api.verify_email(email, code).await?;

        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::AuthApiError;
    use crate::domain::lawyer::CreateLawyerInput;
    use crate::domain::lawyer::Lawyer;
    use crate::domain::lawyer::Role;
    use crate::domain::submission::CreateSubmissionInput;
    use crate::domain::submission::Submission;
    use crate::domain::submission::SubmissionStatus;
    use chrono::Utc;
    use mockall::predicate::*;
    use mockall::*;
    use std::time::Duration;
    use uuid::Uuid;

    mock! {
        pub Presigner {}

        #[async_trait::async_trait]
        impl S3Presigner for Presigner {
            async fn presign_put_object(
                &self,
                key: &str,
                content_type: &str,
                expires_in: Duration,
            ) -> Result<String, PresignerError>;
        }
    }

    mock! {
        pub LawyerRepo {}

        #[async_trait::async_trait]
        impl LawyerRepository for LawyerRepo {
            async fn create_lawyer(&self, input: CreateLawyerInput) -> Result<Lawyer, DatabaseError>;
            async fn get_lawyer(&self, by: UserBy) -> Result<Option<Lawyer>, DatabaseError>;
            async fn exists_by_oab_number(&self, oab_number: &str) -> Result<bool, DatabaseError>;
            async fn verify_email(&self, user_id: Uuid) -> Result<bool, DatabaseError>;
            async fn verify_face(&self, user_id: Uuid) -> Result<bool, DatabaseError>;
        }
    }

    mock! {
        pub SubmissionRepo {}

        #[async_trait::async_trait]
        impl SubmissionRepository for SubmissionRepo {
            async fn create_submission(
                &self,
                input: CreateSubmissionInput,
            ) -> Result<Submission, DatabaseError>;
        }
    }

    mock! {
        pub KafkaMessageProducer {}

        #[async_trait::async_trait]
        impl MessageProducer for KafkaMessageProducer {
            async fn profile_review_requested(
                &self,
                user_id: Uuid,
            ) -> Result<(), ProducerError>;
        }
    }

    mock! {
        pub GrpcAuthApi {}

        #[async_trait::async_trait]
        impl AuthApi for GrpcAuthApi {
            async fn create_credential(&self, email: &str, password: &str) -> Result<Uuid, AuthApiError>;
            async fn request_email_verification(&self, email: &str) -> Result<(), AuthApiError>;
            async fn verify_email(&self, email: &str, code: &str) -> Result<(), AuthApiError>;
        }

    }

    fn mk_service(
        lawyer_repo: Arc<dyn LawyerRepository>,
        submission_repo: Arc<dyn SubmissionRepository>,
        presigner: Arc<dyn S3Presigner>,
        message_producer: Arc<dyn MessageProducer>,
        grpc_auth_api: Arc<dyn AuthApi>,
    ) -> CoreService {
        CoreService::new(
            lawyer_repo,
            submission_repo,
            presigner,
            message_producer,
            grpc_auth_api,
        )
    }

    fn mk_lawyer(id: Uuid, status: Status, email: &str) -> Lawyer {
        Lawyer {
            id,
            status,
            name: "Mario".to_string(),
            email: email.to_string(),
            role: Role::Lawyer,
            email_verified_at: None,
            face_verified_at: None,
            created_at: Utc::now(),
            updated_at: Some(Utc::now()),
            oab_number: "283283".to_string(),
        }
    }

    fn mk_submission(user_id: Uuid) -> Submission {
        Submission {
            id: Uuid::default(),
            user_id,
            status: SubmissionStatus::Pending,
            reviewed_by: None,
            reviewed_at: None,
            reason: String::from(""),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn get_presigned_url_user_not_found() {
        let mut repo = MockLawyerRepo::new();
        repo.expect_get_lawyer()
            .withf(|by| matches!(by, UserBy::Email(e) if e == "a@b.com"))
            .returning(|_| Ok(None));

        let presigner = Arc::new(MockPresigner::new());
        let submission = Arc::new(MockSubmissionRepo::new());
        let producer = Arc::new(MockKafkaMessageProducer::new());
        let grpc_auth_api = Arc::new(MockGrpcAuthApi::new());

        let svc = mk_service(
            Arc::new(repo),
            submission,
            presigner,
            producer,
            grpc_auth_api,
        );

        let err = svc
            .get_presigned_url("a@b.com".to_string())
            .await
            .unwrap_err();
        assert!(matches!(err, CoreServiceError::UserNotFound));
    }

    #[tokio::test]
    async fn get_presigned_url_email_not_verified() {
        let user_id = Uuid::parse_str("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa").unwrap();

        let mut repo = MockLawyerRepo::new();
        repo.expect_get_lawyer().returning(move |_| {
            Ok(Some(Lawyer {
                id: user_id,
                status: Status::PendingEmailVerification,
                name: String::from("Mario"),
                email: String::from("mario@gmail.com"),
                role: Role::Lawyer,
                email_verified_at: None,
                face_verified_at: None,
                created_at: Utc::now(),
                updated_at: Some(Utc::now()),
                oab_number: String::from("283283"),
            }))
        });

        let presigner = Arc::new(MockPresigner::new());
        let submission = Arc::new(MockSubmissionRepo::new());
        let producer = Arc::new(MockKafkaMessageProducer::new());
        let grpc_auth_api = Arc::new(MockGrpcAuthApi::new());

        let svc = mk_service(
            Arc::new(repo),
            submission,
            presigner,
            producer,
            grpc_auth_api,
        );

        let err = svc
            .get_presigned_url("user@example.com".to_string())
            .await
            .unwrap_err();

        assert!(matches!(err, CoreServiceError::UserEmailNotVerified));
    }

    #[tokio::test]
    async fn get_presigned_url_success() {
        let user_id = Uuid::parse_str("bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb").unwrap();

        let mut repo = MockLawyerRepo::new();
        repo.expect_get_lawyer()
            .withf(|by| matches!(by, UserBy::Email(e) if e == "ok@example.com"))
            .returning(move |_| {
                Ok(Some(crate::domain::lawyer::Lawyer {
                    id: user_id,
                    status: Status::EmailVerified,
                    name: String::from("Mario"),
                    email: String::from("mario@gmail.com"),
                    role: Role::Lawyer,
                    email_verified_at: None,
                    face_verified_at: None,
                    created_at: Utc::now(),
                    updated_at: Some(Utc::now()),
                    oab_number: String::from("283283"),
                }))
            });

        let expected_key = format!("user_verification_{}", user_id);
        let expected_url = "https://example.com/presigned".to_string();

        let mut presigner = MockPresigner::new();
        presigner
            .expect_presign_put_object()
            .with(
                eq(expected_key.clone()),
                eq("image/jpeg"),
                eq(Duration::from_secs(300)),
            )
            .returning(move |_, _, _| Ok("https://example.com/presigned".to_string()));

        let submission = Arc::new(MockSubmissionRepo::new());
        let producer = Arc::new(MockKafkaMessageProducer::new());
        let grpc_auth_api = Arc::new(MockGrpcAuthApi::new());
        let svc = mk_service(
            Arc::new(repo),
            submission,
            Arc::new(presigner),
            producer,
            grpc_auth_api,
        );
        let res = svc
            .get_presigned_url("ok@example.com".to_string())
            .await
            .unwrap();

        assert_eq!(res.key, expected_key);
        assert_eq!(res.upload_url, expected_url);
    }

    #[tokio::test]
    async fn get_presigned_url_db_error_maps_to_internal() {
        let mut repo = MockLawyerRepo::new();
        repo.expect_get_lawyer()
            .returning(|_| Err(DatabaseError::Conflict));

        let presigner = Arc::new(MockPresigner::new());
        let submission = Arc::new(MockSubmissionRepo::new());
        let producer = Arc::new(MockKafkaMessageProducer::new());
        let grpc_auth_api = Arc::new(MockGrpcAuthApi::new());
        let svc = mk_service(
            Arc::new(repo),
            submission,
            presigner,
            producer,
            grpc_auth_api,
        );

        let err = svc
            .get_presigned_url("fail@example.com".to_string())
            .await
            .unwrap_err();

        assert!(matches!(err, CoreServiceError::Internal));
    }

    #[tokio::test]
    async fn submit_profile_for_review_invalid_email_format() {
        let mut repo = MockLawyerRepo::new();
        repo.expect_get_lawyer().times(0);
        repo.expect_verify_face().times(0);

        let mut submission_repo = MockSubmissionRepo::new();
        submission_repo.expect_create_submission().times(0);

        let mut producer = MockKafkaMessageProducer::new();
        producer.expect_profile_review_requested().times(0);

        let presigner = Arc::new(MockPresigner::new());
        let grpc_auth_api = Arc::new(MockGrpcAuthApi::new());
        let svc = mk_service(
            Arc::new(repo),
            Arc::new(submission_repo),
            presigner,
            Arc::new(producer),
            grpc_auth_api,
        );

        let err = svc
            .submit_profile_for_review("not-an-email".to_string(), "img_key".to_string())
            .await
            .unwrap_err();

        assert!(matches!(err, CoreServiceError::InvalidEmailFormat));
    }

    #[tokio::test]
    async fn submit_profile_for_review_invalid_image_key_format() {
        let mut repo = MockLawyerRepo::new();
        repo.expect_get_lawyer().times(0);
        repo.expect_verify_face().times(0);

        let mut submission_repo = MockSubmissionRepo::new();
        submission_repo.expect_create_submission().times(0);

        let mut producer = MockKafkaMessageProducer::new();
        producer.expect_profile_review_requested().times(0);
        let grpc_auth_api = Arc::new(MockGrpcAuthApi::new());
        let presigner = Arc::new(MockPresigner::new());
        let svc = mk_service(
            Arc::new(repo),
            Arc::new(submission_repo),
            presigner,
            Arc::new(producer),
            grpc_auth_api,
        );
        let err = svc
            .submit_profile_for_review("ok@example.com".to_string(), "   ".to_string())
            .await
            .unwrap_err();

        assert!(matches!(err, CoreServiceError::InvalidImageKeyFormat));
    }

    #[tokio::test]
    async fn submit_profile_for_review_user_not_found() {
        let mut repo = MockLawyerRepo::new();
        repo.expect_get_lawyer().times(1).returning(|_| Ok(None));
        repo.expect_verify_face().times(0);

        let mut submission_repo = MockSubmissionRepo::new();
        submission_repo.expect_create_submission().times(0);

        let mut producer = MockKafkaMessageProducer::new();
        producer.expect_profile_review_requested().times(0);

        let presigner = Arc::new(MockPresigner::new());
        let grpc_auth_api = Arc::new(MockGrpcAuthApi::new());
        let svc = mk_service(
            Arc::new(repo),
            Arc::new(submission_repo),
            presigner,
            Arc::new(producer),
            grpc_auth_api,
        );

        let err = svc
            .submit_profile_for_review("missing@example.com".to_string(), "img_key".to_string())
            .await
            .unwrap_err();

        assert!(matches!(err, CoreServiceError::UserNotFound));
    }

    #[tokio::test]
    async fn submit_profile_for_review_invalid_status_transition() {
        let user_id = Uuid::new_v4();
        let email = "badstatus@example.com";

        let mut repo = MockLawyerRepo::new();
        repo.expect_get_lawyer().times(1).returning(move |_| {
            Ok(Some(mk_lawyer(
                user_id,
                Status::PendingEmailVerification,
                email,
            )))
        });

        repo.expect_verify_face().times(0);

        let mut submission_repo = MockSubmissionRepo::new();
        submission_repo.expect_create_submission().times(0);

        let mut producer = MockKafkaMessageProducer::new();
        producer.expect_profile_review_requested().times(0);

        let presigner = MockPresigner::new();
        let grpc_auth_api = MockGrpcAuthApi::new();
        let svc = mk_service(
            Arc::new(repo),
            Arc::new(submission_repo),
            Arc::new(presigner),
            Arc::new(producer),
            Arc::new(grpc_auth_api),
        );

        let err = svc
            .submit_profile_for_review(email.to_string(), "img_key".to_string())
            .await
            .unwrap_err();

        assert!(matches!(
            err,
            CoreServiceError::InvalidStatusTransition {
                from: Status::PendingEmailVerification,
                to: Status::PendingFaceReview
            }
        ));
    }

    #[tokio::test]
    async fn submit_profile_for_review_success() {
        let user_id = Uuid::new_v4();
        let email = "ok@example.com";
        let image_key = "user_verification_xxx.jpg";

        let mut repo = MockLawyerRepo::new();
        repo.expect_get_lawyer()
            .times(1)
            .returning(move |_| Ok(Some(mk_lawyer(user_id, Status::EmailVerified, email))));

        repo.expect_verify_face()
            .with(eq(user_id))
            .times(1)
            .returning(|_| Ok(true));

        let mut submission_repo = MockSubmissionRepo::new();
        submission_repo
            .expect_create_submission()
            .times(1)
            .returning(move |_| Ok(mk_submission(user_id)));

        let mut producer = MockKafkaMessageProducer::new();
        producer
            .expect_profile_review_requested()
            .with(eq(user_id))
            .times(1)
            .returning(|_| Ok(()));

        let presigner = MockPresigner::new();
        let grpc_auth_api = MockGrpcAuthApi::new();
        let svc = mk_service(
            Arc::new(repo),
            Arc::new(submission_repo),
            Arc::new(presigner),
            Arc::new(producer),
            Arc::new(grpc_auth_api),
        );

        svc.submit_profile_for_review(email.to_string(), image_key.to_string())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn submit_profile_for_review_verify_face_not_found() {
        let user_id = Uuid::new_v4();
        let email = "ok@example.com";

        let mut repo = MockLawyerRepo::new();
        repo.expect_get_lawyer()
            .times(1)
            .returning(move |_| Ok(Some(mk_lawyer(user_id, Status::EmailVerified, email))));

        repo.expect_verify_face()
            .with(eq(user_id))
            .times(1)
            .returning(|_| Err(DatabaseError::NotFound));

        let mut submission_repo = MockSubmissionRepo::new();
        submission_repo.expect_create_submission().times(0);

        let mut producer = MockKafkaMessageProducer::new();
        producer.expect_profile_review_requested().times(0);

        let presigner = Arc::new(MockPresigner::new());
        let grpc_auth_api = MockGrpcAuthApi::new();
        let svc = mk_service(
            Arc::new(repo),
            Arc::new(submission_repo),
            presigner,
            Arc::new(producer),
            Arc::new(grpc_auth_api),
        );

        let err = svc
            .submit_profile_for_review(email.to_string(), "img_key".to_string())
            .await
            .unwrap_err();

        assert!(matches!(err, CoreServiceError::Internal));
    }

    #[tokio::test]
    async fn submit_profile_for_review_producer() {
        let user_id = Uuid::new_v4();
        let email = "ok@example.com";

        let mut repo = MockLawyerRepo::new();
        repo.expect_get_lawyer()
            .times(1)
            .returning(move |_| Ok(Some(mk_lawyer(user_id, Status::EmailVerified, email))));

        repo.expect_verify_face()
            .with(eq(user_id))
            .times(1)
            .returning(|_| Ok(true));

        let mut submission_repo = MockSubmissionRepo::new();
        submission_repo
            .expect_create_submission()
            .times(1)
            .returning(move |_| Ok(mk_submission(user_id)));

        let mut producer = MockKafkaMessageProducer::new();
        producer
            .expect_profile_review_requested()
            .with(eq(user_id))
            .times(1)
            .returning(|_| Err(ProducerError::Kafka(rdkafka::error::KafkaError::Canceled)));

        let presigner = Arc::new(MockPresigner::new());
        let grpc_auth_api = MockGrpcAuthApi::new();
        let svc = mk_service(
            Arc::new(repo),
            Arc::new(submission_repo),
            presigner,
            Arc::new(producer),
            Arc::new(grpc_auth_api),
        );

        let err = svc
            .submit_profile_for_review(email.to_string(), "img_key".to_string())
            .await
            .unwrap_err();

        assert!(matches!(err, CoreServiceError::Producer(_)));
    }

    #[tokio::test]
    async fn create_lawyer_invalid_email() {
        let repo = Arc::new(MockLawyerRepo::new());
        let presigner = Arc::new(MockPresigner::new());
        let submission = Arc::new(MockSubmissionRepo::new());
        let producer = Arc::new(MockKafkaMessageProducer::new());
        let grpc_auth_api = Arc::new(MockGrpcAuthApi::new());

        let svc = mk_service(repo, submission, presigner, producer, grpc_auth_api);

        let err = svc
            .create_lawyer_account(
                "not-an-email".to_string(),
                "SomeStrongPassword123!".to_string(),
                "283283".to_string(),
                "Mario".to_string(),
            )
            .await
            .unwrap_err();

        assert!(matches!(err, CoreServiceError::InvalidEmailFormat));
    }

    #[tokio::test]
    async fn create_lawyer_invalid_password() {
        let repo = Arc::new(MockLawyerRepo::new());
        let presigner = Arc::new(MockPresigner::new());
        let submission = Arc::new(MockSubmissionRepo::new());
        let producer = Arc::new(MockKafkaMessageProducer::new());
        let grpc_auth_api = Arc::new(MockGrpcAuthApi::new());

        let svc = mk_service(repo, submission, presigner, producer, grpc_auth_api);

        let err = svc
            .create_lawyer_account(
                "a@b.com".to_string(),
                "123".to_string(),
                "283283".to_string(),
                "Mario".to_string(),
            )
            .await
            .unwrap_err();

        assert!(matches!(err, CoreServiceError::InvalidPasswordFormat(_)));
    }

    #[tokio::test]
    async fn create_lawyer_oab_already_exists() {
        let mut repo = MockLawyerRepo::new();
        repo.expect_exists_by_oab_number()
            .times(1)
            .returning(|_| Ok(true));

        let presigner = Arc::new(MockPresigner::new());
        let submission = Arc::new(MockSubmissionRepo::new());
        let producer = Arc::new(MockKafkaMessageProducer::new());
        let grpc_auth_api = Arc::new(MockGrpcAuthApi::new());

        let svc = mk_service(
            Arc::new(repo),
            submission,
            presigner,
            producer,
            grpc_auth_api,
        );

        let err = svc
            .create_lawyer_account(
                "a@b.com".to_string(),
                "SomeStrongPassword123!".to_string(),
                "283283".to_string(),
                "Mario".to_string(),
            )
            .await
            .unwrap_err();

        assert!(matches!(
            err,
            CoreServiceError::LawyerWithOABNumberAlreadyExists
        ));
    }

    #[tokio::test]
    async fn create_lawyer_sucess() {
        let user_id = Uuid::new_v4();
        let lawyer_id = Uuid::new_v4();

        let mut repo = MockLawyerRepo::new();
        repo.expect_exists_by_oab_number()
            .times(1)
            .returning(|_| Ok(false));

        repo.expect_create_lawyer()
            .withf(move |input: &CreateLawyerInput| {
                input.email == "a@b.com"
                    && input.name == "Mario"
                    && input.oab_number == "283283"
                    && input.user_id == user_id
                    && input.status == Status::PendingEmailVerification
                    && input.role == Role::Lawyer
            })
            .times(1)
            .returning(move |_input| {
                Ok(mk_lawyer(
                    lawyer_id,
                    Status::PendingEmailVerification,
                    "a@b.com",
                ))
            });

        let mut grpc_auth_api = MockGrpcAuthApi::new();
        grpc_auth_api
            .expect_create_credential()
            .withf(|email, password| email == "a@b.com" && password == "SomeStrongPassword123!")
            .times(1)
            .returning(move |_, _| Ok(user_id));

        grpc_auth_api
            .expect_request_email_verification()
            .withf(|email| email == "a@b.com")
            .times(1)
            .returning(|_| Ok(()));

        let presigner = Arc::new(MockPresigner::new());
        let submission = Arc::new(MockSubmissionRepo::new());
        let producer = Arc::new(MockKafkaMessageProducer::new());

        let svc = mk_service(
            Arc::new(repo),
            submission,
            presigner,
            producer,
            Arc::new(grpc_auth_api),
        );

        let lawyer = svc
            .create_lawyer_account(
                "a@b.com".to_string(),
                "SomeStrongPassword123!".to_string(),
                "283283".to_string(),
                "Mario".to_string(),
            )
            .await
            .unwrap();

        assert_eq!(lawyer.id, lawyer_id);
        assert_eq!(lawyer.email, "a@b.com");
        assert_eq!(lawyer.status, Status::PendingEmailVerification);
    }

    #[tokio::test]
    async fn create_lawyer_auth_create_credential_error() {
        let mut repo = MockLawyerRepo::new();
        repo.expect_exists_by_oab_number()
            .times(1)
            .returning(|_| Ok(false));

        let mut grpc_auth_api = MockGrpcAuthApi::new();
        grpc_auth_api
            .expect_create_credential()
            .returning(|_, _| Err(AuthApiError::Unavailable));

        let presigner = Arc::new(MockPresigner::new());
        let submission = Arc::new(MockSubmissionRepo::new());
        let producer = Arc::new(MockKafkaMessageProducer::new());

        let svc = mk_service(
            Arc::new(repo),
            submission,
            presigner,
            producer,
            Arc::new(grpc_auth_api),
        );

        let err = svc
            .create_lawyer_account(
                "a@b.com".to_string(),
                "SomeStrongPassword123!".to_string(),
                "283283".to_string(),
                "Mario".to_string(),
            )
            .await
            .unwrap_err();

        assert!(matches!(err, CoreServiceError::AuthServiceUnavailable));
    }

    #[tokio::test]
    async fn create_lawyer_repository_create_error() {
        let user_id = Uuid::new_v4();

        let mut repo = MockLawyerRepo::new();
        repo.expect_exists_by_oab_number()
            .times(1)
            .returning(|_| Ok(false));

        repo.expect_create_lawyer()
            .times(1)
            .returning(|_| Err(DatabaseError::ForeignKeyViolation));

        let mut grpc_auth_api = MockGrpcAuthApi::new();
        grpc_auth_api
            .expect_create_credential()
            .times(1)
            .returning(move |_, _| Ok(user_id));

        let presigner = Arc::new(MockPresigner::new());
        let submission = Arc::new(MockSubmissionRepo::new());
        let producer = Arc::new(MockKafkaMessageProducer::new());

        let svc = mk_service(
            Arc::new(repo),
            submission,
            presigner,
            producer,
            Arc::new(grpc_auth_api),
        );

        let err = svc
            .create_lawyer_account(
                "a@b.com".to_string(),
                "SomeStrongPassword123!".to_string(),
                "283283".to_string(),
                "Mario".to_string(),
            )
            .await
            .unwrap_err();

        assert!(matches!(err, CoreServiceError::Internal));
    }

    #[tokio::test]
    async fn create_lawyer_email_verification_request_error() {
        let user_id = Uuid::new_v4();
        let lawyer_id = Uuid::new_v4();

        let mut repo = MockLawyerRepo::new();
        repo.expect_exists_by_oab_number()
            .times(1)
            .returning(|_| Ok(false));

        repo.expect_create_lawyer()
            .times(1)
            .returning(move |_input| {
                Ok(mk_lawyer(
                    lawyer_id,
                    Status::PendingEmailVerification,
                    "a@b.com",
                ))
            });

        let mut grpc_auth_api = MockGrpcAuthApi::new();
        grpc_auth_api
            .expect_create_credential()
            .times(1)
            .returning(move |_, _| Ok(user_id));

        grpc_auth_api
            .expect_request_email_verification()
            .times(1)
            .returning(|_| Err(AuthApiError::Unavailable));

        let presigner = Arc::new(MockPresigner::new());
        let submission = Arc::new(MockSubmissionRepo::new());
        let producer = Arc::new(MockKafkaMessageProducer::new());

        let svc = mk_service(
            Arc::new(repo),
            submission,
            presigner,
            producer,
            Arc::new(grpc_auth_api),
        );

        let err = svc
            .create_lawyer_account(
                "a@b.com".to_string(),
                "SomeStrongPassword123!".to_string(),
                "283283".to_string(),
                "Mario".to_string(),
            )
            .await
            .unwrap_err();

        assert!(matches!(err, CoreServiceError::AuthServiceUnavailable));
    }

    #[tokio::test]
    async fn request_new_email_verification_code_invalid_email() {
        let repo = Arc::new(MockLawyerRepo::new());
        let presigner = Arc::new(MockPresigner::new());
        let submission = Arc::new(MockSubmissionRepo::new());
        let producer = Arc::new(MockKafkaMessageProducer::new());
        let grpc_auth_api = Arc::new(MockGrpcAuthApi::new());

        let svc = mk_service(repo, submission, presigner, producer, grpc_auth_api);

        let err = svc
            .request_new_email_verifcation_code("invalid-email")
            .await
            .unwrap_err();

        assert!(matches!(err, CoreServiceError::InvalidEmailFormat));
    }

    #[tokio::test]
    async fn request_new_email_verification_code_user_not_found() {
        let mut repo = MockLawyerRepo::new();
        repo.expect_get_lawyer().times(1).returning(|_| Ok(None));

        let presigner = Arc::new(MockPresigner::new());
        let submission = Arc::new(MockSubmissionRepo::new());
        let producer = Arc::new(MockKafkaMessageProducer::new());
        let grpc_auth_api = Arc::new(MockGrpcAuthApi::new()); // must not be called

        let svc = mk_service(
            Arc::new(repo),
            submission,
            presigner,
            producer,
            grpc_auth_api,
        );

        let err = svc
            .request_new_email_verifcation_code("a@b.com")
            .await
            .unwrap_err();

        assert!(matches!(err, CoreServiceError::InvalidCredentials));
    }

    #[tokio::test]
    async fn request_new_email_verification_code() {
        let user_id = Uuid::new_v4();

        let mut repo = MockLawyerRepo::new();
        repo.expect_get_lawyer().times(1).returning(move |_| {
            Ok(Some(mk_lawyer(
                user_id,
                Status::PendingEmailVerification,
                "a@b.com",
            )))
        });

        let mut grpc_auth_api = MockGrpcAuthApi::new();
        grpc_auth_api
            .expect_request_email_verification()
            .times(1)
            .returning(|_| Ok(()));

        let presigner = Arc::new(MockPresigner::new());
        let submission = Arc::new(MockSubmissionRepo::new());
        let producer = Arc::new(MockKafkaMessageProducer::new());

        let svc = mk_service(
            Arc::new(repo),
            submission,
            presigner,
            producer,
            Arc::new(grpc_auth_api),
        );

        let result = svc
            .request_new_email_verifcation_code("a@b.com")
            .await
            .unwrap();

        assert_eq!(result, true);
    }

    #[tokio::test]
    async fn request_new_email_verification_code_repository_error() {
        let mut repo = MockLawyerRepo::new();
        repo.expect_get_lawyer()
            .times(1)
            .returning(|_| Err(DatabaseError::ForeignKeyViolation));

        let presigner = Arc::new(MockPresigner::new());
        let submission = Arc::new(MockSubmissionRepo::new());
        let producer = Arc::new(MockKafkaMessageProducer::new());
        let grpc_auth_api = Arc::new(MockGrpcAuthApi::new());

        let svc = mk_service(
            Arc::new(repo),
            submission,
            presigner,
            producer,
            grpc_auth_api,
        );

        let err = svc
            .request_new_email_verifcation_code("a@b.com")
            .await
            .unwrap_err();

        assert!(matches!(err, CoreServiceError::Internal));
    }

    #[tokio::test]
    async fn request_new_email_verification_code_auth_error() {
        let user_id = Uuid::new_v4();

        let mut repo = MockLawyerRepo::new();
        repo.expect_get_lawyer().times(1).returning(move |_| {
            Ok(Some(mk_lawyer(
                user_id,
                Status::PendingEmailVerification,
                "a@b.com",
            )))
        });

        let mut grpc_auth_api = MockGrpcAuthApi::new();
        grpc_auth_api
            .expect_request_email_verification()
            .times(1)
            .returning(|_| Err(AuthApiError::Unavailable));

        let presigner = Arc::new(MockPresigner::new());
        let submission = Arc::new(MockSubmissionRepo::new());
        let producer = Arc::new(MockKafkaMessageProducer::new());

        let svc = mk_service(
            Arc::new(repo),
            submission,
            presigner,
            producer,
            Arc::new(grpc_auth_api),
        );

        let err = svc
            .request_new_email_verifcation_code("a@b.com")
            .await
            .unwrap_err();

        assert!(matches!(err, CoreServiceError::AuthServiceUnavailable));
    }
}
