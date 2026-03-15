use crate::validation::constraints::{Constraint, ConstraintResult, ConstraintViolation};

/// Password strength level
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PasswordStrength {
    /// At least 6 chars
    Weak,
    /// At least 8 chars, mixed case + digits
    Medium,
    /// At least 10 chars, mixed case + digits + special
    Strong,
    /// At least 12 chars, mixed case + digits + special, no common patterns
    VeryStrong,
}

/// Validates that a password meets security requirements.
///
/// Equivalent to Symfony's `PasswordStrength` constraint.
pub struct Password {
    pub min_strength: PasswordStrength,
    pub message: String,
    pub min_length: usize,
}

impl Default for Password {
    fn default() -> Self {
        Self {
            min_strength: PasswordStrength::Medium,
            message: "The password strength is too low. Please use a stronger password.".to_string(),
            min_length: 8,
        }
    }
}

impl Password {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn strength(mut self, strength: PasswordStrength) -> Self {
        self.min_strength = strength;
        self.min_length = match strength {
            PasswordStrength::Weak => 6,
            PasswordStrength::Medium => 8,
            PasswordStrength::Strong => 10,
            PasswordStrength::VeryStrong => 12,
        };
        self
    }

    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = message.into();
        self
    }

    fn evaluate_strength(value: &str) -> PasswordStrength {
        let len = value.len();
        let has_lower = value.chars().any(|c| c.is_ascii_lowercase());
        let has_upper = value.chars().any(|c| c.is_ascii_uppercase());
        let has_digit = value.chars().any(|c| c.is_ascii_digit());
        let has_special = value.chars().any(|c| !c.is_alphanumeric());

        let common_patterns = [
            "password", "123456", "qwerty", "abc123", "letmein", "admin",
            "welcome", "monkey", "dragon", "master", "azerty",
        ];
        let lower = value.to_lowercase();
        let has_common = common_patterns.iter().any(|p| lower.contains(p));

        if len >= 12 && has_lower && has_upper && has_digit && has_special && !has_common {
            PasswordStrength::VeryStrong
        } else if len >= 10 && has_lower && has_upper && has_digit && has_special {
            PasswordStrength::Strong
        } else if len >= 8 && has_lower && has_upper && has_digit {
            PasswordStrength::Medium
        } else {
            PasswordStrength::Weak
        }
    }
}

impl Constraint for Password {
    fn validate(&self, value: &str) -> ConstraintResult {
        if value.len() < self.min_length {
            return Err(ConstraintViolation::new(
                self.name(),
                format!(
                    "The password must be at least {} characters long.",
                    self.min_length
                ),
                "[REDACTED]",
            ));
        }

        let actual_strength = Self::evaluate_strength(value);
        if actual_strength < self.min_strength {
            return Err(ConstraintViolation::new(
                self.name(),
                &self.message,
                "[REDACTED]",
            ));
        }

        Ok(())
    }

    fn name(&self) -> &'static str {
        "Password"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_weak_password() {
        let constraint = Password::new().strength(PasswordStrength::Weak);
        assert!(constraint.validate("abcdef").is_ok());
        assert!(constraint.validate("12345").is_err()); // too short
    }

    #[test]
    fn test_medium_password() {
        let constraint = Password::new().strength(PasswordStrength::Medium);
        assert!(constraint.validate("Abcdef1x").is_ok());
        assert!(constraint.validate("abcdefgh").is_err()); // no upper/digit
    }

    #[test]
    fn test_strong_password() {
        let constraint = Password::new().strength(PasswordStrength::Strong);
        assert!(constraint.validate("Abcdef1x!z").is_ok());
        assert!(constraint.validate("Abcdef1x").is_err()); // no special, too short
    }

    #[test]
    fn test_very_strong_password() {
        let constraint = Password::new().strength(PasswordStrength::VeryStrong);
        assert!(constraint.validate("MyStr0ng!Pass").is_ok());
        assert!(constraint.validate("password123!A").is_err()); // common pattern
    }

    #[test]
    fn test_too_short() {
        let constraint = Password::new().strength(PasswordStrength::Medium);
        let err = constraint.validate("Ab1!").unwrap_err();
        assert!(err.message.contains("at least"));
    }

    #[test]
    fn test_password_not_leaked_in_error() {
        let constraint = Password::new();
        let err = constraint.validate("weak").unwrap_err();
        assert_eq!(err.invalid_value, "[REDACTED]");
    }
}
