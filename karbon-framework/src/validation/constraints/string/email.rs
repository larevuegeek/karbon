use crate::validation::constraints::{Constraint, ConstraintResult, ConstraintViolation};

/// Validates that a value is a valid email address.
///
/// Equivalent to Symfony's `Email` constraint.
pub struct Email {
    pub message: String,
}

impl Default for Email {
    fn default() -> Self {
        Self {
            message: "This value is not a valid email address.".to_string(),
        }
    }
}

impl Email {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = message.into();
        self
    }

    fn is_valid_email(value: &str) -> bool {
        if value.is_empty() {
            return false;
        }

        let parts: Vec<&str> = value.splitn(2, '@').collect();
        if parts.len() != 2 {
            return false;
        }

        let local = parts[0];
        let domain = parts[1];

        // Local part validation
        if local.is_empty() || local.len() > 64 {
            return false;
        }

        // Domain validation
        if domain.is_empty() || domain.len() > 255 {
            return false;
        }

        // Domain must contain at least one dot
        if !domain.contains('.') {
            return false;
        }

        // Domain parts validation
        for part in domain.split('.') {
            if part.is_empty() || part.len() > 63 {
                return false;
            }
            if part.starts_with('-') || part.ends_with('-') {
                return false;
            }
            if !part.chars().all(|c| c.is_alphanumeric() || c == '-') {
                return false;
            }
        }

        // Local part: allowed characters
        let valid_local = local
            .chars()
            .all(|c| c.is_alphanumeric() || ".!#$%&'*+/=?^_`{|}~-".contains(c));

        if !valid_local {
            return false;
        }

        // No consecutive dots in local part
        if local.contains("..") {
            return false;
        }

        true
    }
}

impl Constraint for Email {
    fn validate(&self, value: &str) -> ConstraintResult {
        if !Self::is_valid_email(value) {
            return Err(ConstraintViolation::new(self.name(), &self.message, value));
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        "Email"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_emails() {
        let constraint = Email::new();
        assert!(constraint.validate("user@example.com").is_ok());
        assert!(constraint.validate("user.name@example.com").is_ok());
        assert!(constraint.validate("user+tag@example.com").is_ok());
        assert!(constraint.validate("user@sub.domain.com").is_ok());
        assert!(constraint.validate("u@example.co").is_ok());
    }

    #[test]
    fn test_invalid_emails() {
        let constraint = Email::new();
        assert!(constraint.validate("").is_err());
        assert!(constraint.validate("not-an-email").is_err());
        assert!(constraint.validate("@example.com").is_err());
        assert!(constraint.validate("user@").is_err());
        assert!(constraint.validate("user@.com").is_err());
        assert!(constraint.validate("user@domain").is_err());
        assert!(constraint.validate("user..name@example.com").is_err());
        assert!(constraint.validate("user@-domain.com").is_err());
    }

    #[test]
    fn test_custom_message() {
        let constraint = Email::new().with_message("Email invalide.");
        let err = constraint.validate("invalid").unwrap_err();
        assert_eq!(err.message, "Email invalide.");
    }
}
