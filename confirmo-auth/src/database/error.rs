use sqlx::Error as SqlxError;

#[derive(Debug)]
pub enum AuthDatabaseError {
    /// Unique constraint violations (email already exists, etc.)
    Conflict,
    /// Row not found
    NotFound,
    /// Any other database error
    Database(SqlxError),
}

impl From<SqlxError> for AuthDatabaseError {
    fn from(err: SqlxError) -> Self {
        tracing::error!("Database error: {:#?}", err);
        match err {
            SqlxError::RowNotFound => AuthDatabaseError::NotFound,
            SqlxError::Database(db_err) => {
                // PostgreSQL unique violation: SQLSTATE 23505
                if db_err.code().as_deref() == Some("23505") {
                    AuthDatabaseError::Conflict
                } else {
                    AuthDatabaseError::Database(SqlxError::Database(db_err))
                }
            }
            other => AuthDatabaseError::Database(other),
        }
    }
}
