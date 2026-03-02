use std::time::Duration;

use confirmo_shared::{
    auth::{
        CreateCredentialsRequest, CreateCredentialsResponse, EmailVerificationRequest,
        VerifyEmailRequest, auth_service_server::AuthService,
    },
    auth_event::{AuthEvent, EmailVerificationPayload},
};
use confirmo_utils::{email::validate_email, password::validate_password};
use rand::Rng;
use rdkafka::{
    message::{Header, OwnedHeaders},
    producer::{FutureProducer, FutureRecord},
};
use serde::{Deserialize, Serialize};
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

const KAKFA_TOPIC_SEND_MESSAGE_TIMEOUT_IN_SECONDS: u64 = 5000;
const EMAIL_VERIFICATION_CODE_DIGITS: usize = 6;
const MAXIMUM_EMAIL_VERIFICATION_ATTEMPTS: u8 = 3;

impl From<AuthDatabaseError> for Status {
    fn from(value: AuthDatabaseError) -> Self {
        tracing::error!("{:?}", value);
        Status::internal("Something went wrong, see the logs")
    }
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
    hash_secret: String,
    producer: FutureProducer,
    kafka_auth_email_verification: String,
    kafka_auth_email_verified: String,
}

impl<C: CredentialsRepository> AuthServer<C> {
    pub fn new(
        credentials_repository: C,
        hash_secret: String,
        producer: FutureProducer,
        kafka_auth_email_verification: String,
        kafka_auth_email_verified: String,
    ) -> Self {
        Self {
            credentials_repository,
            hash_secret,
            producer,
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

        match maybe_credential_verification {
            Some(_) => Ok(Response::new(())),
            None => {
                let code = generate_email_code();
                let code_hash = hash_code(&code, self.hash_secret.as_bytes());
                let expirest_at = Utc::now() + Duration::from_mins(3);

                self.credentials_repository
                    .create_verification_code(credential.id, &code_hash, expirest_at)
                    .await?;

                let payload = AuthEvent::EmailVerification(EmailVerificationPayload {
                    email: credential.email,
                    token: code,
                });

                let payload = serde_json::to_string(&payload).map_err(|e| {
                    tracing::error!(
                        "Error converting payload to string for message to kafka: {:#?}",
                        e
                    );
                    Status::internal("Internal Server Error")
                })?;

                let trace_id = metadata.get(TRACE_ID_HEADER).and_then(|v| v.to_str().ok());
                let header = OwnedHeaders::new().insert(Header {
                    key: TRACE_ID_HEADER,
                    value: trace_id,
                });

                let record: FutureRecord<'_, str, String> =
                    FutureRecord::to(&self.kafka_auth_email_verification)
                        .payload(&payload)
                        .headers(header);

                self.producer
                    .send(
                        record,
                        Duration::from_secs(KAKFA_TOPIC_SEND_MESSAGE_TIMEOUT_IN_SECONDS),
                    )
                    .await
                    .map_err(|e| {
                        tracing::error!(
                            "Error sending message to topic: '{}' error: {:#?}",
                            &self.kafka_auth_email_verification,
                            e
                        );
                        Status::internal("Internal Server Error")
                    })?;

                Ok(Response::new(()))
            }
        }
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
            self.credentials_repository
                .update_credential_verification(verification)
                .await?;

            return Err(Status::not_found("Invalid or expired verification code."));
        }

        let hash = hash_code(&input.code, self.hash_secret.as_bytes());

        if verification.code != hash {
            tracing::error!("Wrong code");
            self.credentials_repository
                .update_credential_verification(verification)
                .await?;

            return Err(Status::not_found("Invalid or expired verification code."));
        }

        verification.used_at = Some(Utc::now());

        let result = self
            .credentials_repository
            .update_credential_verification(verification)
            .await?;

        self.credentials_repository
            .verify_email_credential(&input.email)
            .await?;

        let trace_id = metadata.get(TRACE_ID_HEADER).and_then(|v| v.to_str().ok());
        let header = OwnedHeaders::new().insert(Header {
            key: TRACE_ID_HEADER,
            value: trace_id,
        });

        let payload = CoreEvent::EmailVerified(EmailVerifiedPayload {
            user_id: result.credential_id.to_string(),
        });

        let payload = serde_json::to_string(&payload).map_err(|e| {
            tracing::error!(
                "Error converting payload to string for message to kafka: {:#?}",
                e
            );
            Status::internal("Internal Server Error")
        })?;

        let record: FutureRecord<'_, str, String> =
            FutureRecord::to(&self.kafka_auth_email_verified)
                .payload(&payload)
                .headers(header);

        self.producer
            .send(
                record,
                Duration::from_secs(KAKFA_TOPIC_SEND_MESSAGE_TIMEOUT_IN_SECONDS),
            )
            .await
            .map_err(|e| {
                tracing::error!(
                    "Error sending message to topic: '{}' error: {:#?}",
                    &self.kafka_auth_email_verified,
                    e
                );
                Status::internal("Internal Server Error")
            })?;

        tracing::info!("Email verified successfuly");
        Ok(Response::new(()))
    }
}
