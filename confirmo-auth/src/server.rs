use std::sync::Arc;
use std::time::Duration;

use confirmo_outbox::{NewOutboxMessage, OutboxRepository};
use confirmo_shared::{
    auth::{
        CreateCredentialsRequest, CreateCredentialsResponse, EmailVerificationRequest,
        VerifyEmailRequest, auth_service_server::AuthService,
    },
    auth_event::{AuthEvent, EmailVerificationPayload},
};
use confirmo_utils::{email::validate_email, password::validate_password};
use rand::Rng;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use sqlx::types::chrono::Utc;
use tonic::{Request, Response, Status};

use crate::{
    database::{
        credentials::{CredentialsRepository, UserBy},
        error::AuthDatabaseError,
    },
    error::password_error_to_grpc_status,
    interceptor::TRACE_ID_HEADER,
    password::hash_password,
};

const EMAIL_VERIFICATION_CODE_DIGITS: usize = 6;
const MAXIMUM_EMAIL_VERIFICATION_ATTEMPTS: u8 = 3;

impl From<AuthDatabaseError> for Status {
    fn from(value: AuthDatabaseError) -> Self {
        tracing::error!("{:?}", value);
        Status::internal("Something went wrong, see the logs")
    }
}

fn internal(error: impl std::fmt::Debug) -> Status {
    tracing::error!("{:?}", error);
    Status::internal("Something went wrong, see the logs")
}

fn generate_email_code() -> String {
    let mut rng = rand::rng();

    let code: u32 = rng.random_range(0..1_000_000);

    format!("{:06}", code)
}

use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

pub fn hash_code(code: &str, secret: &[u8]) -> String {
    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC can take key of any size");

    mac.update(code.as_bytes());

    let result = mac.finalize().into_bytes();
    hex::encode(result)
}

#[derive(Debug, Serialize, Deserialize)]
pub enum CoreEvent {
    EmailVerified(EmailVerifiedPayload),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EmailVerifiedPayload {
    pub user_id: String,
}

pub struct AuthServer<C>
where
    C: CredentialsRepository,
{
    credentials_repository: C,
    outbox_repository: Arc<dyn OutboxRepository>,
    pool: Arc<PgPool>,
    hash_secret: String,
    kafka_auth_email_verification: String,
    kafka_auth_email_verified: String,
}

impl<C: CredentialsRepository> AuthServer<C> {
    pub fn new(
        credentials_repository: C,
        outbox_repository: Arc<dyn OutboxRepository>,
        pool: Arc<PgPool>,
        hash_secret: String,
        kafka_auth_email_verification: String,
        kafka_auth_email_verified: String,
    ) -> Self {
        Self {
            credentials_repository,
            outbox_repository,
            pool,
            hash_secret,
            kafka_auth_email_verification,
            kafka_auth_email_verified,
        }
    }
}

#[tonic::async_trait]
impl<C: CredentialsRepository> AuthService for AuthServer<C> {
    async fn create_credentials(
        &self,
        request: Request<CreateCredentialsRequest>,
    ) -> Result<Response<CreateCredentialsResponse>, Status> {
        let body = request.into_inner();

        if !validate_email(&body.email) {
            return Err(Status::invalid_argument("Invalid Email Format"));
        }

        validate_password(&body.password).map_err(password_error_to_grpc_status)?;

        let user_exists = self.credentials_repository.exists(&body.email).await?;

        if user_exists {
            tracing::error!("User with email {} already exists", body.email);
            return Err(Status::already_exists("Email Already Registered"));
        }

        let password_hash = hash_password(&body.password).map_err(|e| {
            tracing::error!("Error hashing the password: {}", e);
            Status::internal("Internal Server Error")
        })?;

        let credential = self
            .credentials_repository
            .create_credential(body.email, password_hash)
            .await?;

        let response = CreateCredentialsResponse {
            user_id: credential.id.to_string(),
        };

        tracing::info!(
            "Credential created successfuly with id '{:?}'",
            response.user_id
        );

        Ok(Response::new(response))
    }

    async fn request_email_verification(
        &self,
        request: Request<EmailVerificationRequest>,
    ) -> Result<Response<()>, Status> {
        let metadata = request.metadata().clone();
        let input = request.into_inner();
        let email = input.email;

        if !validate_email(&email) {
            return Err(Status::invalid_argument("Invalid Email Format"));
        }

        let credential = self
            .credentials_repository
            .get_credential(UserBy::Email(email))
            .await?
            .ok_or_else(|| Status::not_found("Credential not found"))?;

        if credential.email_verified {
            return Ok(Response::new(()));
        }

        let maybe_credential_verification = self
            .credentials_repository
            .get_lastest_verification_code_by_credential_id(credential.id)
            .await?;

        if maybe_credential_verification.is_some() {
            return Ok(Response::new(()));
        }

        let credential_id = credential.id;
        let code = generate_email_code();
        let code_hash = hash_code(&code, self.hash_secret.as_bytes());
        let expires_at = Utc::now() + Duration::from_mins(3);

        let trace_id = metadata
            .get(TRACE_ID_HEADER)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);

        let event = AuthEvent::EmailVerification(EmailVerificationPayload {
            email: credential.email,
            token: code,
        });
        let payload = serde_json::to_value(&event).map_err(internal)?;

        let mut tx = self.pool.begin().await.map_err(internal)?;

        self.credentials_repository
            .create_verification_code(&mut tx, credential_id, &code_hash, expires_at)
            .await?;

        self.outbox_repository
            .enqueue(
                &mut tx,
                NewOutboxMessage {
                    topic: self.kafka_auth_email_verification.clone(),
                    partition_key: None,
                    payload,
                    trace_id,
                },
            )
            .await
            .map_err(internal)?;

        tx.commit().await.map_err(internal)?;

        Ok(Response::new(()))
    }

    async fn verify_email(
        &self,
        request: Request<VerifyEmailRequest>,
    ) -> Result<Response<()>, Status> {
        let metadata = request.metadata().clone();
        let input = request.into_inner();

        if input.code.len() < EMAIL_VERIFICATION_CODE_DIGITS {
            tracing::error!("Email verification code shorter than expected");
            return Err(Status::not_found("Invalid or expired verification code."));
        }

        if !validate_email(&input.email) {
            return Err(Status::invalid_argument("Invalid Email Format"));
        };

        let mut verification = self
            .credentials_repository
            .get_lastest_verification_code_by_email(input.email.clone())
            .await?
            .ok_or_else(|| Status::not_found("Invalid or expired verification code."))?;

        verification.attempt_count = verification.attempt_count.saturating_add(1);

        if verification.attempt_count > MAXIMUM_EMAIL_VERIFICATION_ATTEMPTS as i32
            || verification.expires_at < Utc::now()
        {
            tracing::error!("Email verification code exceeded attempt count or expired");
            let mut conn = self.pool.acquire().await.map_err(internal)?;
            self.credentials_repository
                .update_credential_verification(&mut conn, verification)
                .await?;

            return Err(Status::not_found("Invalid or expired verification code."));
        }

        let hash = hash_code(&input.code, self.hash_secret.as_bytes());

        if verification.code != hash {
            tracing::error!("Wrong code");
            let mut conn = self.pool.acquire().await.map_err(internal)?;
            self.credentials_repository
                .update_credential_verification(&mut conn, verification)
                .await?;

            return Err(Status::not_found("Invalid or expired verification code."));
        }

        verification.used_at = Some(Utc::now());
        let credential_id = verification.credential_id;

        let trace_id = metadata
            .get(TRACE_ID_HEADER)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);

        let event = CoreEvent::EmailVerified(EmailVerifiedPayload {
            user_id: credential_id.to_string(),
        });
        let payload = serde_json::to_value(&event).map_err(internal)?;

        let mut tx = self.pool.begin().await.map_err(internal)?;

        self.credentials_repository
            .update_credential_verification(&mut tx, verification)
            .await?;

        self.credentials_repository
            .verify_email_credential(&mut tx, &input.email)
            .await?;

        self.outbox_repository
            .enqueue(
                &mut tx,
                NewOutboxMessage {
                    topic: self.kafka_auth_email_verified.clone(),
                    partition_key: None,
                    payload,
                    trace_id,
                },
            )
            .await
            .map_err(internal)?;

        tx.commit().await.map_err(internal)?;

        tracing::info!("Email verified successfuly");
        Ok(Response::new(()))
    }
}
