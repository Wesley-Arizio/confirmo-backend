use std::sync::Arc;

use sqlx::{
    PgPool,
    types::{
        Uuid,
        chrono::{DateTime, Utc},
    },
};

use crate::database::error::AuthDatabaseError;

#[derive(sqlx::FromRow, Debug, PartialEq, Eq)]
pub struct Credentials {
    pub id: Uuid,
    pub email: String,
    pub password_hash: String,
    pub email_verified: bool,
    pub login_allowed: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow, Debug, PartialEq, Eq)]
pub struct CredentialVerification {
    pub id: Uuid,
    pub code: String,
    pub credential_id: Uuid,
    pub attempt_count: i32,
    pub expires_at: DateTime<Utc>,
    pub used_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[mockall::automock]
#[async_trait::async_trait]
pub trait CredentialsRepository: Send + Sync + 'static {
    async fn create_credential(
        &self,
        email: String,
        password_hash: String,
    ) -> Result<Credentials, AuthDatabaseError>;

    async fn verify_email_credential(&self, email: &str) -> Result<(), AuthDatabaseError>;

    async fn get_credential(&self, id: UserBy) -> Result<Option<Credentials>, AuthDatabaseError>;

    async fn exists(&self, email: &str) -> Result<bool, AuthDatabaseError>;

    async fn create_verification_code(
        &self,
        credential_id: Uuid,
        code: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<CredentialVerification, AuthDatabaseError>;
    async fn get_lastest_verification_code_by_credential_id(
        &self,
        credential_id: Uuid,
    ) -> Result<Option<CredentialVerification>, AuthDatabaseError>;
    async fn get_lastest_verification_code_by_email(
        &self,
        email: String,
    ) -> Result<Option<CredentialVerification>, AuthDatabaseError>;

    async fn update_credential_verification(
        &self,
        credential_verification: CredentialVerification,
    ) -> Result<CredentialVerification, AuthDatabaseError>;
}

pub struct CredentialsPostgresRepo {
    pg_pool: Arc<PgPool>,
}

impl CredentialsPostgresRepo {
    pub fn new(pg_pool: Arc<PgPool>) -> Self {
        Self { pg_pool }
    }
}

#[derive(Debug)]
pub enum UserBy {
    Id(Uuid),
    Email(String),
}

#[async_trait::async_trait]
impl CredentialsRepository for CredentialsPostgresRepo {
    async fn create_credential(
        &self,
        email: String,
        password_hash: String,
    ) -> Result<Credentials, AuthDatabaseError> {
        let credential: Credentials = sqlx::query_as(r#"INSERT INTO credentials (email, password_hash) VALUES ($1, $2) RETURNING id, email, password_hash, email_verified, login_allowed, created_at, updated_at"#).bind(email).bind(password_hash).fetch_one(&*self.pg_pool)
        .await?;

        Ok(credential)
    }

    async fn exists(&self, email: &str) -> Result<bool, AuthDatabaseError> {
        let exists =
            sqlx::query_scalar(r#"SELECT EXISTS (SELECT 1 FROM credentials WHERE email = $1)"#)
                .bind(email)
                .fetch_one(&*self.pg_pool)
                .await?;

        Ok(exists)
    }

    async fn get_credential(
        &self,
        user_by: UserBy,
    ) -> Result<Option<Credentials>, AuthDatabaseError> {
        let maybe_credential: Option<Credentials> = match user_by {
            UserBy::Id(id) => {
                sqlx::query_as(r#"SELECT id, email, password_hash, email_verified, login_allowed, created_at, updated_at FROM credentials WHERE id = $1"#)
                .bind(id)
                .fetch_optional(&*self.pg_pool)
                .await?
            }
            UserBy::Email(email) => {
                sqlx::query_as(r#"SELECT id, email, password_hash, email_verified, login_allowed, created_at, updated_at FROM credentials WHERE email = $1"#)
                .bind(email)
                .fetch_optional(&*self.pg_pool)
                .await?
            }
        };

        Ok(maybe_credential)
    }

    async fn create_verification_code(
        &self,
        credential_id: Uuid,
        code: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<CredentialVerification, AuthDatabaseError> {
        let credential_verification: CredentialVerification = sqlx::query_as(r#"INSERT INTO credential_verifications (credential_id, code, expires_at) VALUES ($1, $2, $3) RETURNING id, code, expires_at, used_at, created_at, credential_id, attempt_count"#).bind(credential_id).bind(code).bind(expires_at).fetch_one(&*self.pg_pool)
        .await?;

        Ok(credential_verification)
    }

    async fn get_lastest_verification_code_by_credential_id(
        &self,
        credential_id: Uuid,
    ) -> Result<Option<CredentialVerification>, AuthDatabaseError> {
        let maybe_credential_verification: Option<CredentialVerification> = sqlx::query_as(
            r#"
            SELECT 
                id,
                code,
                expires_at,
                used_at,
                created_at,
                credential_id,
                attempt_count
            FROM credential_verifications
            WHERE credential_id = $1
                AND used_at IS NULL
                AND expires_at > now()
            ORDER BY created_at DESC
            LIMIT 1;
        "#,
        )
        .bind(credential_id)
        .fetch_optional(&*self.pg_pool)
        .await?;

        Ok(maybe_credential_verification)
    }

    async fn get_lastest_verification_code_by_email(
        &self,
        email: String,
    ) -> Result<Option<CredentialVerification>, AuthDatabaseError> {
        let maybe_credential_verification: Option<CredentialVerification> = sqlx::query_as(
            r#"
            SELECT 
                cv.id, 
                cv.code, 
                cv.expires_at, 
                cv.used_at, 
                cv.created_at, 
                cv.credential_id,
                cv.attempt_count
            FROM credential_verifications cv
            JOIN credentials c 
                ON c.id = cv.credential_id
            WHERE c.email = $1 
                AND c.email_verified = FALSE
                AND cv.used_at IS NULL 
                AND cv.expires_at > now()
            ORDER BY cv.created_at DESC
            LIMIT 1;
        "#,
        )
        .bind(email)
        .fetch_optional(&*self.pg_pool)
        .await?;

        Ok(maybe_credential_verification)
    }

    async fn update_credential_verification(
        &self,
        credential_verification: CredentialVerification,
    ) -> Result<CredentialVerification, AuthDatabaseError> {
        let credential_verification: CredentialVerification = sqlx::query_as(r#"UPDATE credential_verifications SET used_at = $2, attempt_count = $3 WHERE id = $1 RETURNING id, code, expires_at, used_at, created_at, credential_id, attempt_count"#)
            .bind(credential_verification.id)
            .bind(credential_verification.used_at)
            .bind(credential_verification.attempt_count)
            .fetch_one(&*self.pg_pool)
        .await?;

        Ok(credential_verification)
    }

    async fn verify_email_credential(&self, email: &str) -> Result<(), AuthDatabaseError> {
        let result = sqlx::query(
            r#"UPDATE credentials SET email_verified = true, updated_at = now() WHERE email = $1 AND email_verified = false"#,
        )
        .bind(email)
        .execute(&*self.pg_pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(AuthDatabaseError::NotFound);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {}
