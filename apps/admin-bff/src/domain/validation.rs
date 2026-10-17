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

pub fn validate_plan_name(name: &str) -> Result<(), BffError> {
    validate_string_length(name, 100, "name")
}

pub fn validate_plan_slug(slug: &str) -> Result<(), BffError> {
    if slug.is_empty() {
        return Err(BffError::Validation("slug is required".into()));
    }
    if slug.len() > 50 {
        return Err(BffError::Validation(
            "slug must not exceed 50 characters".into(),
        ));
    }
    if !slug
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(BffError::Validation(
            "slug must only contain lowercase letters, digits, and hyphens".into(),
        ));
    }
    Ok(())
}

pub fn validate_decimal(s: &str, field: &str) -> Result<(), BffError> {
    if s.is_empty() {
        return Err(BffError::Validation(format!("{field} is required")));
    }
    let _: f64 = s
        .parse()
        .map_err(|_| BffError::Validation(format!("{field} must be a valid decimal number")))?;
    Ok(())
}

pub fn validate_non_negative_int(value: i32, field: &str) -> Result<(), BffError> {
    if value < 0 {
        return Err(BffError::Validation(format!(
            "{field} must be non-negative"
        )));
    }
    Ok(())
}

pub fn validate_model_name(name: &str) -> Result<(), BffError> {
    validate_string_length(name, 200, "public_model_name")
}

pub fn validate_provider_name(name: &str) -> Result<(), BffError> {
    validate_string_length(name, 100, "name")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_valid() {
        assert_eq!(
            validate_email("admin@example.com").unwrap(),
            "admin@example.com"
        );
    }

    #[test]
    fn email_empty() {
        assert!(validate_email("").is_err());
    }

    #[test]
    fn email_no_at() {
        assert!(validate_email("notanemail").is_err());
    }

    #[test]
    fn email_no_dot() {
        assert!(validate_email("admin@example").is_err());
    }

    #[test]
    fn email_too_long() {
        let long = "a".repeat(250) + "@b.com";
        assert!(validate_email(&long).is_err());
    }

    #[test]
    fn password_valid() {
        assert!(validate_password("Abcd1234!xyz").is_ok());
    }

    #[test]
    fn password_too_short() {
        assert!(validate_password("Ab1!").is_err());
    }

    #[test]
    fn password_no_uppercase() {
        assert!(validate_password("abcd1234!xyz").is_err());
    }

    #[test]
    fn password_no_special() {
        assert!(validate_password("Abcd1234xyz").is_err());
    }

    #[test]
    fn uuid_valid() {
        let u = "550e8400-e29b-41d4-a716-446655440000";
        assert_eq!(
            validate_uuid(u, "test_id").unwrap(),
            Uuid::parse_str(u).unwrap()
        );
    }

    #[test]
    fn uuid_invalid() {
        assert!(validate_uuid("not-a-uuid", "test_id").is_err());
    }

    #[test]
    fn string_length_valid() {
        assert!(validate_string_length("hello", 10, "field").is_ok());
    }

    #[test]
    fn string_length_empty() {
        assert!(validate_string_length("", 10, "field").is_err());
    }

    #[test]
    fn string_length_exceeds() {
        assert!(validate_string_length("hello world", 5, "field").is_err());
    }

    #[test]
    fn mfa_code_valid() {
        assert!(validate_mfa_code("123456").is_ok());
    }

    #[test]
    fn mfa_code_empty() {
        assert!(validate_mfa_code("").is_err());
    }

    #[test]
    fn plan_slug_valid() {
        assert!(validate_plan_slug("basic-plan").is_ok());
        assert!(validate_plan_slug("pro2024").is_ok());
    }

    #[test]
    fn plan_slug_invalid_chars() {
        assert!(validate_plan_slug("Basic Plan").is_err());
        assert!(validate_plan_slug("plan_1").is_err());
    }

    #[test]
    fn decimal_valid() {
        assert!(validate_decimal("10.50", "amount").is_ok());
        assert!(validate_decimal("0", "amount").is_ok());
    }

    #[test]
    fn decimal_invalid() {
        assert!(validate_decimal("abc", "amount").is_err());
        assert!(validate_decimal("", "amount").is_err());
    }

    #[test]
    fn non_negative_int_valid() {
        assert!(validate_non_negative_int(0, "rate").is_ok());
        assert!(validate_non_negative_int(100, "rate").is_ok());
    }

    #[test]
    fn non_negative_int_negative() {
        assert!(validate_non_negative_int(-1, "rate").is_err());
    }
}
