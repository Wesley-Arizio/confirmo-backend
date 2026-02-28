use sqlx::{Error as SqlxError, error::ErrorKind};

#[derive(Debug)]
pub enum DatabaseError {
    Conflict,
    NotFound,
    ForeignKeyViolation,
    Database(SqlxError),
}

impl From<SqlxError> for DatabaseError {
    fn from(err: SqlxError) -> Self {
        match err {
            SqlxError::RowNotFound => DatabaseError::NotFound,
            SqlxError::Database(db_err) => match db_err.kind() {
                ErrorKind::UniqueViolation => DatabaseError::Conflict,
                ErrorKind::ForeignKeyViolation => DatabaseError::ForeignKeyViolation,
                _ => DatabaseError::Database(SqlxError::Database(db_err)),
            },
            other => DatabaseError::Database(other),
        }
    }
}
