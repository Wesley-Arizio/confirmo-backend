use std::sync::Arc;

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use sqlx::{prelude::Type, types::Uuid};

use crate::database::error::DatabaseError;
use crate::database::firm::FirmDAO;
use crate::database::traits::{LawyerRepository, UserBy};
use crate::domain::lawyer::{CreateLawyerInput, Lawyer};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Type)]
#[sqlx(type_name = "status", rename_all = "snake_case")]
pub enum StatusDAO {
    PendingEmailVerification,
    EmailVerified,
    Active,
    Suspended,
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Type)]
#[sqlx(type_name = "role", rename_all = "snake_case")]
pub enum RoleDAO {
    Lawyer,
    Client,
    Admin,
}

#[derive(sqlx::FromRow, Debug, PartialEq, Eq)]
pub struct UserDAO {
    pub id: Uuid,
    pub name: String,
    pub email: String,
    pub status: StatusDAO,
    pub role: RoleDAO,
    pub email_verified_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(sqlx::FromRow, Debug, PartialEq, Eq)]
pub struct LawyerDAO {
    pub user_id: Uuid,
    pub oab_number: String,
    pub firm_id: Uuid,
}

pub struct LawyerPostgresRepository {
    pub pg_pool: Arc<PgPool>,
}

impl LawyerPostgresRepository {
    pub fn new(pg_pool: Arc<PgPool>) -> Self {
        Self { pg_pool }
    }
}

#[async_trait::async_trait]
impl LawyerRepository for LawyerPostgresRepository {
    async fn exists_by_oab_number(&self, oab_number: &str) -> Result<bool, DatabaseError> {
        let exists: bool = sqlx::query_scalar(
            r#"SELECT EXISTS (SELECT 1 FROM lawyers WHERE oab_number = $1)::bool"#,
        )
        .bind(oab_number)
        .fetch_one(&*self.pg_pool)
        .await?;

        Ok(exists)
    }

    async fn get_lawyer(&self, by: UserBy) -> Result<Option<Lawyer>, DatabaseError> {
        let maybe_user: Option<UserDAO> = match by {
            UserBy::Id(id) => {
                sqlx::query_as(r#"SELECT id, name, email, status, role, email_verified_at, created_at, updated_at FROM users WHERE id = $1"#)
                    .bind(id)
                    .fetch_optional(&*self.pg_pool)
                    .await?
            }
            UserBy::Email(email) => {
                sqlx::query_as(r#"SELECT id, name, email, status, role, email_verified_at, created_at, updated_at FROM users WHERE email = $1"#)
                    .bind(email)
                    .fetch_optional(&*self.pg_pool)
                    .await?
            }
        };

        if let Some(user) = maybe_user {
            let lawyer: LawyerDAO = sqlx::query_as(
                r#"SELECT user_id, oab_number, firm_id FROM lawyers WHERE user_id = $1"#,
            )
            .bind(user.id)
            .fetch_one(&*self.pg_pool)
            .await?;

            let lawyer = Lawyer::from_dao(user, lawyer);

            return Ok(Some(lawyer));
        };

        Ok(None)
    }

    async fn create_lawyer(&self, input: CreateLawyerInput) -> Result<Lawyer, DatabaseError> {
        let mut tx = self.pg_pool.begin().await?;

        let firm: FirmDAO = sqlx::query_as(
            r#"INSERT INTO firms (name) VALUES ($1) RETURNING id, name, created_at, updated_at;"#,
        )
        .bind(input.firm_name)
        .fetch_one(&mut *tx)
        .await?;

        let user: UserDAO = sqlx::query_as(r#"INSERT INTO users (id, name, email, status, role) VALUES ($1, $2, $3, $4, $5) RETURNING id, name, email, status, role, email_verified_at, created_at, updated_at;"#)
            .bind(input.user_id)
            .bind(input.name)
            .bind(input.email)
            .bind(StatusDAO::from(input.status))
            .bind(RoleDAO::from(input.role))
            .fetch_one(&mut *tx)
            .await?;

        let lawyer: LawyerDAO = sqlx::query_as(
            r#"INSERT INTO lawyers (user_id, oab_number, firm_id) VALUES ($1, $2, $3) RETURNING user_id, oab_number, firm_id;"#,
        )
        .bind(input.user_id)
        .bind(input.oab_number)
        .bind(firm.id)
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;

        let lawyer = Lawyer::from_dao(user, lawyer);

        Ok(lawyer)
    }

    async fn verify_email(&self, user_id: Uuid) -> Result<bool, DatabaseError> {
        let result = sqlx::query(
            r#"UPDATE users SET status = $2::status, email_verified_at = now(), updated_at = now() WHERE id = $1"#,

        )
        .bind(user_id)
        .bind(StatusDAO::EmailVerified)
        .execute(&*self.pg_pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(DatabaseError::NotFound);
        }

        Ok(true)
    }
}
