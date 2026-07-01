use crate::validation::constraints::{Constraint, ConstraintResult, ConstraintViolation};

/// Validates that a value is a valid date string.
///
/// Equivalent to Symfony's `Date` constraint.
/// Default format: YYYY-MM-DD
pub struct Date {
    pub message: String,
    pub format: String,
}

impl Default for Date {
    fn default() -> Self {
        Self {
            message: "This value is not a valid date.".to_string(),
            format: "%Y-%m-%d".to_string(),
        }
    }
}

impl Date {
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
}

impl Constraint for Date {
    fn validate(&self, value: &str) -> ConstraintResult {
        if chrono::NaiveDate::parse_from_str(value, &self.format).is_err() {
            return Err(ConstraintViolation::new(self.name(), &self.message, value));
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        "Date"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_dates() {
        let constraint = Date::new();
        assert!(constraint.validate("2024-01-01").is_ok());
        assert!(constraint.validate("2024-12-31").is_ok());
        assert!(constraint.validate("2024-02-29").is_ok()); // leap year
        assert!(constraint.validate("1999-06-15").is_ok());
    }

    #[test]
    fn test_invalid_dates() {
        let constraint = Date::new();
        assert!(constraint.validate("").is_err());
        assert!(constraint.validate("not-a-date").is_err());
        assert!(constraint.validate("2024-13-01").is_err()); // invalid month
        assert!(constraint.validate("2024-02-30").is_err()); // invalid day
        assert!(constraint.validate("2023-02-29").is_err()); // not a leap year
        assert!(constraint.validate("01-01-2024").is_err()); // wrong format
    }

    #[test]
    fn test_custom_format() {
        let constraint = Date::new().with_format("%d/%m/%Y");
        assert!(constraint.validate("01/01/2024").is_ok());
        assert!(constraint.validate("31/12/2024").is_ok());
        assert!(constraint.validate("2024-01-01").is_err());
    }
}
