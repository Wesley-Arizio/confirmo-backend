use std::sync::Arc;

use confirmo_shared::constants::EMAIL_VERIFICATION_CODE_LENGTH;
use confirmo_utils::{email::validate_email, password::validate_password};
use thiserror::Error;
use uuid::Uuid;

use crate::{
    application::ports::{AuthApi, AuthApiError},
    database::{
        error::DatabaseError,
        traits::{LawyerRepository, UserBy},
    },
    domain::lawyer::{CreateLawyerInput, Lawyer, Role, Status},
};

#[derive(Debug, Error)]
pub enum CoreServiceError {
    #[error("User not found")]
    UserNotFound,

    #[error("Internal error")]
    Internal,

    #[error("Invalid email format")]
    InvalidEmailFormat,

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
    grpc_auth_api: Arc<dyn AuthApi>,
}

impl CoreService {
    pub fn new(
        lawyer_repository: Arc<dyn LawyerRepository>,
        grpc_auth_api: Arc<dyn AuthApi>,
    ) -> Self {
        Self {
            lawyer_repository,
            grpc_auth_api,
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

        let Some(_user) = self
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
    use chrono::Utc;
    use mockall::predicate::*;
    use mockall::*;
    use uuid::Uuid;

    mock! {
        pub LawyerRepo {}

        #[async_trait::async_trait]
        impl LawyerRepository for LawyerRepo {
            async fn create_lawyer(&self, input: CreateLawyerInput) -> Result<Lawyer, DatabaseError>;
            async fn get_lawyer(&self, by: UserBy) -> Result<Option<Lawyer>, DatabaseError>;
            async fn exists_by_oab_number(&self, oab_number: &str) -> Result<bool, DatabaseError>;
            async fn verify_email(&self, user_id: Uuid) -> Result<bool, DatabaseError>;
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
        grpc_auth_api: Arc<dyn AuthApi>,
    ) -> CoreService {
        CoreService::new(lawyer_repo, grpc_auth_api)
    }

    fn mk_lawyer(id: Uuid, status: Status, email: &str) -> Lawyer {
        Lawyer {
            id,
            status,
            name: "Mario".to_string(),
            email: email.to_string(),
            role: Role::Lawyer,
            email_verified_at: None,
            created_at: Utc::now(),
            updated_at: Some(Utc::now()),
            oab_number: "283283".to_string(),
        }
    }

    #[tokio::test]
    async fn create_lawyer_invalid_email() {
        let repo = Arc::new(MockLawyerRepo::new());
        let grpc_auth_api = Arc::new(MockGrpcAuthApi::new());

        let svc = mk_service(repo, grpc_auth_api);

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
        let grpc_auth_api = Arc::new(MockGrpcAuthApi::new());

        let svc = mk_service(repo, grpc_auth_api);

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

        let grpc_auth_api = Arc::new(MockGrpcAuthApi::new());

        let svc = mk_service(Arc::new(repo), grpc_auth_api);

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

        let svc = mk_service(Arc::new(repo), Arc::new(grpc_auth_api));

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

        let svc = mk_service(Arc::new(repo), Arc::new(grpc_auth_api));

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

        let svc = mk_service(Arc::new(repo), Arc::new(grpc_auth_api));

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

        let svc = mk_service(Arc::new(repo), Arc::new(grpc_auth_api));

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
        let grpc_auth_api = Arc::new(MockGrpcAuthApi::new());

        let svc = mk_service(repo, grpc_auth_api);

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

        let grpc_auth_api = Arc::new(MockGrpcAuthApi::new()); // must not be called

        let svc = mk_service(Arc::new(repo), grpc_auth_api);

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

        let svc = mk_service(Arc::new(repo), Arc::new(grpc_auth_api));

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

        let grpc_auth_api = Arc::new(MockGrpcAuthApi::new());

        let svc = mk_service(Arc::new(repo), grpc_auth_api);

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

        let svc = mk_service(Arc::new(repo), Arc::new(grpc_auth_api));

        let err = svc
            .request_new_email_verifcation_code("a@b.com")
            .await
            .unwrap_err();

        assert!(matches!(err, CoreServiceError::AuthServiceUnavailable));
    }
}
