use crate::domain::errors::BffError;
use uuid::Uuid;

pub fn validate_email(email: &str) -> Result<String, BffError> {
    let trimmed = email.trim().to_lowercase();
    if trimmed.is_empty() {
        return Err(BffError::Validation("email is required".into()));
    }
    let parts: Vec<&str> = trimmed.split('@').collect();
    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
        return Err(BffError::Validation("invalid email format".into()));
    }
    if !parts[1].contains('.') {
        return Err(BffError::Validation("invalid email domain".into()));
    }
    if trimmed.len() > 254 {
        return Err(BffError::Validation("email exceeds maximum length".into()));
    }
    Ok(trimmed)
}

pub fn validate_password(password: &str) -> Result<(), BffError> {
    if password.len() < 12 {
        return Err(BffError::Validation(
            "password must have at least 12 characters".into(),
        ));
    }
    if password.len() > 128 {
        return Err(BffError::Validation(
            "password must not exceed 128 characters".into(),
        ));
    }
    let has_upper = password.chars().any(|ch| ch.is_ascii_uppercase());
    let has_lower = password.chars().any(|ch| ch.is_ascii_lowercase());
    let has_digit = password.chars().any(|ch| ch.is_ascii_digit());
    let has_special = password.chars().any(|ch| !ch.is_ascii_alphanumeric());
    if !has_upper || !has_lower || !has_digit || !has_special {
        return Err(BffError::Validation(
            "password must include at least 1 uppercase, 1 lowercase, 1 digit, and 1 special character".into(),
        ));
    }
    Ok(())
}

pub fn validate_uuid(s: &str, field: &str) -> Result<Uuid, BffError> {
    Uuid::parse_str(s)
        .map_err(|_| BffError::Validation(format!("invalid {field}: not a valid UUID")))
}

pub fn validate_amount(s: &str) -> Result<(), BffError> {
    if s.is_empty() {
        return Err(BffError::Validation("amount is required".into()));
    }
    let value: f64 = s
        .parse()
        .map_err(|_| BffError::Validation("amount must be a valid number".into()))?;
    if value <= 0.0 {
        return Err(BffError::Validation(
            "amount must be greater than zero".into(),
        ));
    }
    Ok(())
}

pub fn validate_string_length(s: &str, max: usize, field: &str) -> Result<(), BffError> {
    if s.is_empty() {
        return Err(BffError::Validation(format!("{field} is required")));
    }
    if s.len() > max {
        return Err(BffError::Validation(format!(
            "{field} must not exceed {max} characters"
        )));
    }
    Ok(())
}

pub fn validate_mfa_code(code: &str) -> Result<(), BffError> {
    if code.is_empty() {
        return Err(BffError::Validation("MFA code is required".into()));
    }
    if code.len() > 10 {
        return Err(BffError::Validation(
            "MFA code must not exceed 10 characters".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- validate_email --

    #[test]
    fn email_valid() {
        assert_eq!(
            validate_email("User@Example.com").unwrap(),
            "user@example.com"
        );
    }

    #[test]
    fn email_empty() {
        assert!(
            matches!(validate_email(""), Err(BffError::Validation(m)) if m == "email is required")
        );
    }

    #[test]
    fn email_no_at() {
        assert!(
            matches!(validate_email("notanemail"), Err(BffError::Validation(m)) if m == "invalid email format")
        );
    }

    #[test]
    fn email_empty_local() {
        assert!(
            matches!(validate_email("@domain.com"), Err(BffError::Validation(m)) if m == "invalid email format")
        );
    }

    #[test]
    fn email_empty_domain() {
        assert!(
            matches!(validate_email("user@"), Err(BffError::Validation(m)) if m == "invalid email format")
        );
    }

    #[test]
    fn email_no_dot_in_domain() {
        assert!(
            matches!(validate_email("user@domain"), Err(BffError::Validation(m)) if m == "invalid email domain")
        );
    }

    #[test]
    fn email_too_long() {
        let long = format!("{}@example.com", "a".repeat(246));
        assert!(
            matches!(validate_email(&long), Err(BffError::Validation(m)) if m == "email exceeds maximum length")
        );
    }

    // -- validate_password --

    #[test]
    fn password_valid() {
        assert!(validate_password("Abcd1234!xyz").is_ok());
    }

    #[test]
    fn password_too_short() {
        assert!(matches!(
            validate_password("Ab1!s"),
            Err(BffError::Validation(_))
        ));
    }

    #[test]
    fn password_too_long() {
        let long = "A".repeat(129);
        assert!(matches!(
            validate_password(&long),
            Err(BffError::Validation(_))
        ));
    }

    #[test]
    fn password_no_uppercase() {
        assert!(matches!(
            validate_password("abcd1234!xyz"),
            Err(BffError::Validation(_))
        ));
    }

    #[test]
    fn password_no_lowercase() {
        assert!(matches!(
            validate_password("ABCD1234!XYZ"),
            Err(BffError::Validation(_))
        ));
    }

    #[test]
    fn password_no_digit() {
        assert!(matches!(
            validate_password("Abcdefgh!xyz"),
            Err(BffError::Validation(_))
        ));
    }

    #[test]
    fn password_no_special() {
        assert!(matches!(
            validate_password("Abcdefgh1xyz"),
            Err(BffError::Validation(_))
        ));
    }

    // -- validate_uuid --

    #[test]
    fn uuid_valid() {
        let u = validate_uuid("550e8400-e29b-41d4-a716-446655440000", "test").unwrap();
        assert_eq!(u.to_string(), "550e8400-e29b-41d4-a716-446655440000");
    }

    #[test]
    fn uuid_invalid() {
        assert!(
            matches!(validate_uuid("not-a-uuid", "test"), Err(BffError::Validation(m)) if m == "invalid test: not a valid UUID")
        );
    }

    // -- validate_amount --

    #[test]
    fn amount_valid() {
        assert!(validate_amount("10.50").is_ok());
    }

    #[test]
    fn amount_empty() {
        assert!(
            matches!(validate_amount(""), Err(BffError::Validation(m)) if m == "amount is required")
        );
    }

    #[test]
    fn amount_not_a_number() {
        assert!(
            matches!(validate_amount("abc"), Err(BffError::Validation(m)) if m == "amount must be a valid number")
        );
    }

    #[test]
    fn amount_zero() {
        assert!(
            matches!(validate_amount("0"), Err(BffError::Validation(m)) if m == "amount must be greater than zero")
        );
    }

    #[test]
    fn amount_negative() {
        assert!(
            matches!(validate_amount("-5"), Err(BffError::Validation(m)) if m == "amount must be greater than zero")
        );
    }

    // -- validate_string_length --

    #[test]
    fn string_length_valid() {
        assert!(validate_string_length("hello", 10, "field").is_ok());
    }

    #[test]
    fn string_length_empty() {
        assert!(
            matches!(validate_string_length("", 10, "field"), Err(BffError::Validation(m)) if m == "field is required")
        );
    }

    #[test]
    fn string_length_exceeds_max() {
        assert!(
            matches!(validate_string_length("hello world", 5, "field"), Err(BffError::Validation(m)) if m == "field must not exceed 5 characters")
        );
    }

    // -- validate_mfa_code --

    #[test]
    fn mfa_code_valid() {
        assert!(validate_mfa_code("123456").is_ok());
    }

    #[test]
    fn mfa_code_empty() {
        assert!(
            matches!(validate_mfa_code(""), Err(BffError::Validation(m)) if m == "MFA code is required")
        );
    }

    #[test]
    fn mfa_code_too_long() {
        assert!(
            matches!(validate_mfa_code("12345678901"), Err(BffError::Validation(m)) if m == "MFA code must not exceed 10 characters")
        );
    }
}
