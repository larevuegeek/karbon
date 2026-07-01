use crate::validation::constraints::{Constraint, ConstraintResult, ConstraintViolation};

/// Validates that a value matches a regular expression pattern.
///
/// Equivalent to Symfony's `Regex` constraint.
pub struct Regex {
    pub pattern: regex::Regex,
    pub message: String,
    pub should_match: bool,
}

impl Regex {
    /// Build from a **literal** pattern. Panics on an invalid pattern — only use with a
    /// compile-time authored regex. For a runtime/user-supplied pattern, use [`try_new`].
    ///
    /// [`try_new`]: Regex::try_new
    pub fn new(pattern: &str) -> Self {
        Self::try_new(pattern).expect("Invalid regex pattern")
    }

    /// Fallible constructor: returns `Err` instead of panicking on an invalid pattern.
    /// Use this whenever the pattern is not a hard-coded literal.
    pub fn try_new(pattern: &str) -> Result<Self, regex::Error> {
        Ok(Self {
            pattern: regex::Regex::new(pattern)?,
            message: "This value is not valid.".to_string(),
            should_match: true,
        })
    }

    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = message.into();
        self
    }

    /// If set to false, the constraint will fail when the value matches the pattern.
    pub fn should_match(mut self, should_match: bool) -> Self {
        self.should_match = should_match;
        self
    }
}

impl Constraint for Regex {
    fn validate(&self, value: &str) -> ConstraintResult {
        let matches = self.pattern.is_match(value);

        if matches != self.should_match {
            return Err(ConstraintViolation::new(self.name(), &self.message, value));
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        "Regex"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matching_pattern() {
        let constraint = Regex::new(r"^\d{5}$"); // French postal code
        assert!(constraint.validate("75001").is_ok());
        assert!(constraint.validate("12345").is_ok());
        assert!(constraint.validate("1234").is_err());
        assert!(constraint.validate("123456").is_err());
        assert!(constraint.validate("abcde").is_err());
    }

    #[test]
    fn test_not_matching() {
        let constraint = Regex::new(r"<script").should_match(false);
        assert!(constraint.validate("hello world").is_ok());
        assert!(
            constraint
                .validate("<script>alert('xss')</script>")
                .is_err()
        );
    }

    #[test]
    fn test_complex_pattern() {
        let constraint = Regex::new(r"^[A-Z]{2}-\d{3}$");
        assert!(constraint.validate("FR-123").is_ok());
        assert!(constraint.validate("US-456").is_ok());
        assert!(constraint.validate("fr-123").is_err());
        assert!(constraint.validate("FRA-123").is_err());
    }

    #[test]
    fn test_custom_message() {
        let constraint = Regex::new(r"^\d+$").with_message("Seuls les chiffres sont autorisés.");
        let err = constraint.validate("abc").unwrap_err();
        assert_eq!(err.message, "Seuls les chiffres sont autorisés.");
    }
}
