use crate::validation::constraints::{Constraint, ConstraintResult, ConstraintViolation};

/// Validates that a string length is within a given range.
///
/// Equivalent to Symfony's `Length` constraint.
pub struct Length {
    pub min: Option<usize>,
    pub max: Option<usize>,
    pub exact: Option<usize>,
    pub min_message: String,
    pub max_message: String,
    pub exact_message: String,
}

impl Default for Length {
    fn default() -> Self {
        Self {
            min: None,
            max: None,
            exact: None,
            min_message: "This value is too short. It should have {{ limit }} characters or more."
                .to_string(),
            max_message: "This value is too long. It should have {{ limit }} characters or less."
                .to_string(),
            exact_message: "This value should have exactly {{ limit }} characters.".to_string(),
        }
    }
}

impl Length {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn min(mut self, min: usize) -> Self {
        self.min = Some(min);
        self
    }

    pub fn max(mut self, max: usize) -> Self {
        self.max = Some(max);
        self
    }

    pub fn exact(mut self, exact: usize) -> Self {
        self.exact = Some(exact);
        self
    }

    pub fn with_min_message(mut self, message: impl Into<String>) -> Self {
        self.min_message = message.into();
        self
    }

    pub fn with_max_message(mut self, message: impl Into<String>) -> Self {
        self.max_message = message.into();
        self
    }

    pub fn with_exact_message(mut self, message: impl Into<String>) -> Self {
        self.exact_message = message.into();
        self
    }

    fn format_message(&self, template: &str, limit: usize) -> String {
        template.replace("{{ limit }}", &limit.to_string())
    }
}

impl Constraint for Length {
    fn validate(&self, value: &str) -> ConstraintResult {
        let len = value.chars().count();

        if let Some(exact) = self.exact {
            if len != exact {
                return Err(ConstraintViolation::new(
                    self.name(),
                    self.format_message(&self.exact_message, exact),
                    value,
                ));
            }
            return Ok(());
        }

        if let Some(min) = self.min {
            if len < min {
                return Err(ConstraintViolation::new(
                    self.name(),
                    self.format_message(&self.min_message, min),
                    value,
                ));
            }
        }

        if let Some(max) = self.max {
            if len > max {
                return Err(ConstraintViolation::new(
                    self.name(),
                    self.format_message(&self.max_message, max),
                    value,
                ));
            }
        }

        Ok(())
    }

    fn name(&self) -> &'static str {
        "Length"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_min_length() {
        let constraint = Length::new().min(3);
        assert!(constraint.validate("abc").is_ok());
        assert!(constraint.validate("abcd").is_ok());
        assert!(constraint.validate("ab").is_err());
        assert!(constraint.validate("").is_err());
    }

    #[test]
    fn test_max_length() {
        let constraint = Length::new().max(5);
        assert!(constraint.validate("abc").is_ok());
        assert!(constraint.validate("abcde").is_ok());
        assert!(constraint.validate("abcdef").is_err());
    }

    #[test]
    fn test_min_max_range() {
        let constraint = Length::new().min(2).max(5);
        assert!(constraint.validate("ab").is_ok());
        assert!(constraint.validate("abcde").is_ok());
        assert!(constraint.validate("a").is_err());
        assert!(constraint.validate("abcdef").is_err());
    }

    #[test]
    fn test_exact_length() {
        let constraint = Length::new().exact(4);
        assert!(constraint.validate("abcd").is_ok());
        assert!(constraint.validate("abc").is_err());
        assert!(constraint.validate("abcde").is_err());
    }

    #[test]
    fn test_unicode_characters() {
        let constraint = Length::new().min(2).max(5);
        assert!(constraint.validate("café").is_ok()); // 4 chars
        assert!(constraint.validate("é").is_err()); // 1 char
    }

    #[test]
    fn test_error_message_formatting() {
        let constraint = Length::new().min(3);
        let err = constraint.validate("ab").unwrap_err();
        assert!(err.message.contains("3"));
    }
}
