use crate::validation::constraints::{Constraint, ConstraintResult, ConstraintViolation};

/// Validates that a value is a valid date-time string.
///
/// Equivalent to Symfony's `DateTime` constraint.
/// Default format: YYYY-MM-DD HH:MM:SS
pub struct DateTime {
    pub message: String,
    pub format: String,
}

impl Default for DateTime {
    fn default() -> Self {
        Self {
            message: "This value is not a valid datetime.".to_string(),
            format: "%Y-%m-%d %H:%M:%S".to_string(),
        }
    }
}

impl DateTime {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = message.into();
        self
    }

    pub fn with_format(mut self, format: impl Into<String>) -> Self {
        self.format = format.into();
        self
    }

    /// Accept ISO 8601 format (e.g., 2024-01-01T12:00:00)
    pub fn iso8601() -> Self {
        Self {
            format: "%Y-%m-%dT%H:%M:%S".to_string(),
            ..Self::default()
        }
    }
}

impl Constraint for DateTime {
    fn validate(&self, value: &str) -> ConstraintResult {
        if chrono::NaiveDateTime::parse_from_str(value, &self.format).is_err() {
            return Err(ConstraintViolation::new(self.name(), &self.message, value));
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        "DateTime"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_datetimes() {
        let constraint = DateTime::new();
        assert!(constraint.validate("2024-01-01 00:00:00").is_ok());
        assert!(constraint.validate("2024-12-31 23:59:59").is_ok());
        assert!(constraint.validate("2024-06-15 14:30:00").is_ok());
    }

    #[test]
    fn test_invalid_datetimes() {
        let constraint = DateTime::new();
        assert!(constraint.validate("").is_err());
        assert!(constraint.validate("2024-01-01").is_err()); // missing time
        assert!(constraint.validate("not-a-datetime").is_err());
        assert!(constraint.validate("2024-01-01 25:00:00").is_err()); // invalid hour
    }

    #[test]
    fn test_iso8601_format() {
        let constraint = DateTime::iso8601();
        assert!(constraint.validate("2024-01-01T12:00:00").is_ok());
        assert!(constraint.validate("2024-01-01 12:00:00").is_err());
    }

    #[test]
    fn test_custom_format() {
        let constraint = DateTime::new().with_format("%d/%m/%Y %H:%M");
        assert!(constraint.validate("01/01/2024 12:00").is_ok());
        assert!(constraint.validate("2024-01-01 12:00:00").is_err());
    }
}
