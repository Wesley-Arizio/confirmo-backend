use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor, address::AddressError,
    message::Mailbox, transport::smtp::authentication::Credentials,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AuthEventError {
    #[error("failed to send email")]
    Send(#[from] lettre::transport::smtp::Error),

    #[error("failed to build email message")]
    MessageBuild(#[from] lettre::error::Error),

    #[error("invalid email address")]
    InvalidEmail(#[from] AddressError),
}
pub struct AuthEventConsumer {
    mailer: AsyncSmtpTransport<Tokio1Executor>,
    from: String,
}

impl AuthEventConsumer {
    pub fn new(email: String, from: String, password: String) -> Result<Self, AuthEventError> {
        let credentials = Credentials::new(email, password);
        let mailer = AsyncSmtpTransport::<Tokio1Executor>::relay("smtp.gmail.com")?
            .credentials(credentials)
            .build();
        Ok(Self { mailer, from })
    }

    pub async fn send_email_verification(
        &self,
        email: &str,
        token: &str,
    ) -> Result<(), AuthEventError> {
        let from: Mailbox = self
            .from
            .parse()
            .map_err(|e| AuthEventError::InvalidEmail(e))?;
        let to: Mailbox = email.parse().map_err(|e| AuthEventError::InvalidEmail(e))?;

        let message = Message::builder()
            .from(from)
            .to(to)
            .subject("Confirm your account")
            .body(format!("Code to verify your account: {}", token))
            .map_err(|e| AuthEventError::MessageBuild(e))?;

        self.mailer.send(message).await?;

        Ok(())
    }
}
