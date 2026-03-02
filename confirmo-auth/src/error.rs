use confirmo_utils::password::PasswordError;
use tonic::Status;

pub fn password_error_to_grpc_status(value: PasswordError) -> Status {
    match value {
        PasswordError::TooShort => {
            Status::invalid_argument("Password must be at least 8 characters".to_string())
        }
        PasswordError::MissingLowercase => {
            Status::invalid_argument("Password must contain a lowercase letter".to_string())
        }
        PasswordError::MissingUppercase => {
            Status::invalid_argument("Password must contain an uppercase letter".to_string())
        }
        PasswordError::MissingDigit => {
            Status::invalid_argument("Password must contain a number".to_string())
        }
        PasswordError::MissingSpecialChar => {
            Status::invalid_argument("Password must contain a special character".to_string())
        }
    }
}
