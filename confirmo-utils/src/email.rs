use once_cell::sync::Lazy;
use regex::Regex;

static EMAIL_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^[A-Za-z0-9._%+-]+@(?:[A-Za-z0-9-]+\.)+[A-Za-z]{2,}$").expect("valid email regex")
});

pub fn validate_email(email: &str) -> bool {
    EMAIL_REGEX.is_match(email)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_email() {
        assert!(validate_email("test@example.com"));
    }

    #[test]
    fn invalid_emails() {
        let invalid_emails = vec![
            "",
            "not-an-email",
            "user@",
            "@example.com",
            "user@example",
            "user@example.c",
            "user@@example.com",
            "user example@example.com",
            " user@example.com",
            "user@example.com ",
            "user@example..com",
        ];

        for email in invalid_emails {
            assert!(!validate_email(email), "Expected '{}' to be invalid", email);
        }
    }
}
