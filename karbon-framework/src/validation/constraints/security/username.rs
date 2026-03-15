use crate::validation::constraints::{Constraint, ConstraintResult, ConstraintViolation};

/// Validates that a value is a valid username.
///
/// Rules:
/// - Only alphanumeric, underscores, hyphens, and dots
/// - Must start with a letter or digit
/// - Cannot end with a special character
/// - No consecutive special characters
/// - Length between min and max
pub struct Username {
    pub message: String,
    pub min_length: usize,
    pub max_length: usize,
    pub allow_dots: bool,
    pub allow_hyphens: bool,
}

impl Default for Username {
    fn default() -> Self {
        Self {
            message: "This value is not a valid username.".to_string(),
            min_length: 3,
            max_length: 32,
            allow_dots: true,
            allow_hyphens: true,
        }
    }
}

impl Username {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = message.into();
        self
    }

    pub fn min_length(mut self, min: usize) -> Self {
        self.min_length = min;
        self
    }

    pub fn max_length(mut self, max: usize) -> Self {
        self.max_length = max;
        self
    }

    pub fn allow_dots(mut self, allow: bool) -> Self {
        self.allow_dots = allow;
        self
    }

    pub fn allow_hyphens(mut self, allow: bool) -> Self {
        self.allow_hyphens = allow;
        self
    }

    fn is_special(c: char) -> bool {
        c == '_' || c == '.' || c == '-'
    }

    fn is_valid_username(&self, value: &str) -> bool {
        let len = value.len();

        if len < self.min_length || len > self.max_length {
            return false;
        }

        let chars: Vec<char> = value.chars().collect();

        // Must start with alphanumeric
        if !chars[0].is_alphanumeric() {
            return false;
        }

        // Must end with alphanumeric
        if !chars[len - 1].is_alphanumeric() {
            return false;
        }

        for (i, &c) in chars.iter().enumerate() {
            if c.is_alphanumeric() {
                continue;
            }

            if c == '.' && !self.allow_dots {
                return false;
            }

            if c == '-' && !self.allow_hyphens {
                return false;
            }

            if !c.is_alphanumeric() && c != '_' && c != '.' && c != '-' {
                return false;
            }

            // No consecutive special characters
            if i > 0 && Self::is_special(chars[i - 1]) {
                return false;
            }
        }

        true
    }
}

impl Constraint for Username {
    fn validate(&self, value: &str) -> ConstraintResult {
        if !self.is_valid_username(value) {
            return Err(ConstraintViolation::new(
                self.name(),
                &self.message,
                value,
            ));
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        "Username"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_usernames() {
        let constraint = Username::new();
        assert!(constraint.validate("john").is_ok());
        assert!(constraint.validate("john_doe").is_ok());
        assert!(constraint.validate("john.doe").is_ok());
        assert!(constraint.validate("john-doe").is_ok());
        assert!(constraint.validate("user123").is_ok());
        assert!(constraint.validate("a1b").is_ok());
    }

    #[test]
    fn test_invalid_usernames() {
        let constraint = Username::new();
        assert!(constraint.validate("").is_err());
        assert!(constraint.validate("ab").is_err()); // too short
        assert!(constraint.validate("_john").is_err()); // starts with special
        assert!(constraint.validate("john_").is_err()); // ends with special
        assert!(constraint.validate("john__doe").is_err()); // consecutive specials
        assert!(constraint.validate("john..doe").is_err()); // consecutive specials
        assert!(constraint.validate("john doe").is_err()); // space
        assert!(constraint.validate("john@doe").is_err()); // invalid char
    }

    #[test]
    fn test_no_dots_allowed() {
        let constraint = Username::new().allow_dots(false);
        assert!(constraint.validate("john_doe").is_ok());
        assert!(constraint.validate("john.doe").is_err());
    }

    #[test]
    fn test_no_hyphens_allowed() {
        let constraint = Username::new().allow_hyphens(false);
        assert!(constraint.validate("john_doe").is_ok());
        assert!(constraint.validate("john-doe").is_err());
    }

    #[test]
    fn test_length_constraints() {
        let constraint = Username::new().min_length(5).max_length(10);
        assert!(constraint.validate("johnd").is_ok());
        assert!(constraint.validate("john").is_err());
        assert!(constraint.validate("johndoe1234").is_err());
    }
}
