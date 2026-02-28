use std::fmt;

#[derive(Debug, PartialEq)]
pub enum PasswordError {
    TooShort,
    MissingLowercase,
    MissingUppercase,
    MissingDigit,
    MissingSpecialChar,
}

impl fmt::Display for PasswordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            PasswordError::TooShort => "Password must be at least 8 characters",
            PasswordError::MissingLowercase => "Password must contain a lowercase letter",
            PasswordError::MissingUppercase => "Password must contain an uppercase letter",
            PasswordError::MissingDigit => "Password must contain a number",
            PasswordError::MissingSpecialChar => "Password must contain a special character",
        };
        write!(f, "{msg}")
    }
}

pub fn validate_password(password: &str) -> Result<(), PasswordError> {
    if password.len() < 8 {
        return Err(PasswordError::TooShort);
    }

    let mut has_lower = false;
    let mut has_upper = false;
    let mut has_digit = false;
    let mut has_special = false;

    for c in password.bytes() {
        if c.is_ascii_lowercase() {
            has_lower = true;
        } else if c.is_ascii_uppercase() {
            has_upper = true;
        } else if c.is_ascii_digit() {
            has_digit = true;
        } else if !c.is_ascii_alphanumeric() {
            has_special = true;
        }
    }

    if !has_lower {
        return Err(PasswordError::MissingLowercase);
    }
    if !has_upper {
        return Err(PasswordError::MissingUppercase);
    }
    if !has_digit {
        return Err(PasswordError::MissingDigit);
    }
    if !has_special {
        return Err(PasswordError::MissingSpecialChar);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_password_passes() {
        let result = validate_password("Abcdef1!");
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn password_too_short() {
        let result = validate_password("Ab1!");
        assert_eq!(result, Err(PasswordError::TooShort));
    }

    #[test]
    fn missing_lowercase() {
        let result = validate_password("ABCDEFG1!");
        assert_eq!(result, Err(PasswordError::MissingLowercase));
    }

    #[test]
    fn missing_uppercase() {
        let result = validate_password("abcdefg1!");
        assert_eq!(result, Err(PasswordError::MissingUppercase));
    }

    #[test]
    fn missing_digit() {
        let result = validate_password("Abcdefg!");
        assert_eq!(result, Err(PasswordError::MissingDigit));
    }

    #[test]
    fn missing_special_character() {
        let result = validate_password("Abcdefg1");
        assert_eq!(result, Err(PasswordError::MissingSpecialChar));
    }
}
