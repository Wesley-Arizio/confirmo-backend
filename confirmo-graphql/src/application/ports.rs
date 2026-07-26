use async_trait::async_trait;
use confirmo_shared::auth::{
    CreateCredentialsRequest, EmailVerificationRequest, VerifyEmailRequest,
    auth_service_client::AuthServiceClient,
};
use std::str::FromStr;
use tonic::{Code, Request, Status, transport::Channel};
use uuid::Uuid;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AuthApiError {
    #[error("Parse user id failed: '{0}'")]
    ParseUserIdFailed(#[source] uuid::Error),

    #[error("Auth service unavailable")]
    Unavailable,

    #[error("Invalid credentials")]
    InvalidCredentials,

    #[error("Email already exists")]
    EmailAlreadyExists,

    #[error("Invalid field: '{0}'")]
    InvalidArgument(String),

    #[error("Auth service error: {0}")]
    Grpc(Status),
}

impl From<tonic::Status> for AuthApiError {
    fn from(status: tonic::Status) -> Self {
        let message = status.message();
        match status.code() {
            Code::Unavailable => AuthApiError::Unavailable,
            Code::AlreadyExists => AuthApiError::EmailAlreadyExists,
            Code::Unauthenticated | Code::PermissionDenied => AuthApiError::InvalidCredentials,
            Code::InvalidArgument => AuthApiError::InvalidArgument(message.to_owned()),

            _ => AuthApiError::Grpc(status),
        }
    }
}

#[async_trait]
pub trait AuthApi: Send + Sync {
    async fn create_credential(&self, email: &str, password: &str) -> Result<Uuid, AuthApiError>;
    async fn request_email_verification(&self, email: &str) -> Result<(), AuthApiError>;
    async fn verify_email(&self, email: &str, code: &str) -> Result<(), AuthApiError>;
}

pub struct GrpcAuthApi {
    client: AuthServiceClient<Channel>,
}

impl GrpcAuthApi {
    pub fn new(client: AuthServiceClient<Channel>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl AuthApi for GrpcAuthApi {
    async fn create_credential(&self, email: &str, password: &str) -> Result<Uuid, AuthApiError> {
        let request = tonic::Request::new(CreateCredentialsRequest {
            email: email.to_owned(),
            password: password.to_owned(),
        });

        let mut client = self.client.clone();

        let response = client.create_credentials(request).await?;
        let credential = response.into_inner();
        let user_id =
            Uuid::from_str(&credential.user_id).map_err(AuthApiError::ParseUserIdFailed)?;
        Ok(user_id)
    }

    async fn request_email_verification(&self, email: &str) -> Result<(), AuthApiError> {
        let request = tonic::Request::new(EmailVerificationRequest {
            email: email.to_owned(),
        });

        let mut client = self.client.clone();
        let _ = client.request_email_verification(request).await?;

        Ok(())
    }

    async fn verify_email(&self, email: &str, code: &str) -> Result<(), AuthApiError> {
        let request = Request::new(VerifyEmailRequest {
            email: email.to_owned(),
            code: code.to_owned(),
        });

        let mut client = self.client.clone();
        let _ = client.verify_email(request).await?;

        Ok(())
    }
}
