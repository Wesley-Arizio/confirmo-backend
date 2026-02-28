use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub enum AuthEvent {
    EmailVerification(EmailVerificationPayload),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EmailVerificationPayload {
    pub email: String,
    pub token: String,
}
