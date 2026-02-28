use async_trait::async_trait;
use aws_config::{BehaviorVersion, Region};
use aws_sdk_s3::{
    Client, config::http::HttpResponse, error::SdkError, operation::put_object::PutObjectError,
    presigning::PresigningConfigError,
};
use confirmo_shared::auth::{
    CreateCredentialsRequest, EmailVerificationRequest, VerifyEmailRequest,
    auth_service_client::AuthServiceClient,
};
use rdkafka::{
    ClientConfig,
    producer::{FutureProducer, FutureRecord},
};
use std::{str::FromStr, time::Duration};
use tonic::{Code, Request, Status, transport::Channel};
use uuid::Uuid;

use crate::{CoreEvent, ProfileReviewRequestedPayload};

const KAKFA_TOPIC_SEND_MESSAGE_TIMEOUT_IN_SECONDS: u64 = 5000;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum PresignerError {
    #[error("Invalid presign expiry")]
    InvalidExpiry(#[source] Box<PresigningConfigError>),

    #[error("Failed to presign put_object")]
    PresignFailed(#[source] Box<SdkError<PutObjectError, HttpResponse>>),
}

#[async_trait]
pub trait S3Presigner: Send + Sync {
    async fn presign_put_object(
        &self,
        key: &str,
        content_type: &str,
        expires_in: Duration,
    ) -> Result<String, PresignerError>;
}

pub struct AwsS3Presigner {
    client: Client,
    aws_bucket: String,
}

impl AwsS3Presigner {
    pub async fn new(aws_bucket: String, aws_region: String) -> Self {
        let aws_region = Region::new(aws_region);

        let shared_config = aws_config::defaults(BehaviorVersion::latest())
            .region(aws_region)
            .load()
            .await;
        let client = Client::new(&shared_config);
        Self { client, aws_bucket }
    }
}

#[async_trait]
impl S3Presigner for AwsS3Presigner {
    async fn presign_put_object(
        &self,
        key: &str,
        content_type: &str,
        expires_in: Duration,
    ) -> Result<String, PresignerError> {
        let presigned = aws_sdk_s3::presigning::PresigningConfig::expires_in(expires_in)
            .map_err(|e| PresignerError::InvalidExpiry(Box::new(e)))?;

        let req = self
            .client
            .put_object()
            .bucket(&self.aws_bucket)
            .key(key)
            .content_type(content_type)
            .presigned(presigned)
            .await
            .map_err(|e| PresignerError::PresignFailed(Box::new(e)))?;

        Ok(req.uri().to_owned())
    }
}

#[derive(Debug, Error)]
pub enum ProducerError {
    #[error("Failed to serialize kafka payload")]
    Serialize(#[source] serde_json::Error),

    #[error("Kafka produce error")]
    Kafka(#[source] rdkafka::error::KafkaError),
}

#[async_trait]
pub trait MessageProducer: Send + Sync {
    async fn profile_review_requested(&self, user_id: Uuid) -> Result<(), ProducerError>;
}

pub struct KafkaMessageProducer {
    producer: FutureProducer,
    kafka_profile_review_requested: String,
}

impl KafkaMessageProducer {
    pub fn new(kafka_brokers: &str, kafka_profile_review_requested: String) -> Self {
        let producer: FutureProducer = ClientConfig::new()
            .set("bootstrap.servers", kafka_brokers)
            .set("queue.buffering.max.ms", "0") // Do not buffer
            .create()
            .expect("Producer creation failed");
        Self {
            producer,
            kafka_profile_review_requested,
        }
    }
}

#[async_trait]
impl MessageProducer for KafkaMessageProducer {
    async fn profile_review_requested(&self, user_id: Uuid) -> Result<(), ProducerError> {
        let payload = CoreEvent::ProfileReviewRequested(ProfileReviewRequestedPayload {
            user_id: user_id.to_string(),
        });

        let payload = serde_json::to_string(&payload).map_err(ProducerError::Serialize)?;

        let record: FutureRecord<'_, str, String> =
            FutureRecord::to(&self.kafka_profile_review_requested).payload(&payload);

        if let Err(e) = self
            .producer
            .send(
                record,
                Duration::from_secs(KAKFA_TOPIC_SEND_MESSAGE_TIMEOUT_IN_SECONDS),
            )
            .await
        {
            tracing::error!(
                "Error publishing message to '{}', error: {:#?}",
                &self.kafka_profile_review_requested,
                e.0
            );
            return Err(ProducerError::Kafka(e.0));
        }

        Ok(())
    }
}

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
