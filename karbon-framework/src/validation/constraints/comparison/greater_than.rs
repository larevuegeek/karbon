use crate::validation::constraints::{ConstraintResult, ConstraintViolation, NumericConstraint};

/// Validates that a value is greater than a given number.
///
/// Equivalent to Symfony's `GreaterThan` constraint.
pub struct GreaterThan {
    pub value: f64,
    pub message: String,
    pub or_equal: bool,
}

impl GreaterThan {
    pub fn new(value: f64) -> Self {
        Self {
            value,
            message: "This value should be greater than {{ compared_value }}.".to_string(),
            or_equal: false,
        }
    }

    /// GreaterThanOrEqual variant
    pub fn or_equal(value: f64) -> Self {
        Self {
            value,
            message: "This value should be greater than or equal to {{ compared_value }}."
                .to_string(),
            or_equal: true,
        }
    }

    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = message.into();
        self
    }
}

impl NumericConstraint for GreaterThan {
    fn validate_f64(&self, value: f64) -> ConstraintResult {
        let valid = if self.or_equal {
            value >= self.value
        } else {
            value > self.value
        };

        if !valid {
            return Err(ConstraintViolation::new(
                self.name(),
                self.message
                    .replace("{{ compared_value }}", &self.value.to_string()),
                value.to_string(),
            ));
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        "GreaterThan"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_greater_than() {
        let constraint = GreaterThan::new(10.0);
        assert!(constraint.validate_f64(11.0).is_ok());
        assert!(constraint.validate_f64(100.0).is_ok());
        assert!(constraint.validate_f64(10.0).is_err()); // not strictly greater
        assert!(constraint.validate_f64(9.0).is_err());
    }

    #[test]
    fn test_greater_than_or_equal() {
        let constraint = GreaterThan::or_equal(10.0);
        assert!(constraint.validate_f64(10.0).is_ok());
        assert!(constraint.validate_f64(11.0).is_ok());
        assert!(constraint.validate_f64(9.0).is_err());
    }

    #[test]
    fn test_with_negative_values() {
        let constraint = GreaterThan::new(-5.0);
        assert!(constraint.validate_f64(0.0).is_ok());
        assert!(constraint.validate_f64(-4.0).is_ok());
        assert!(constraint.validate_f64(-6.0).is_err());
    }
}
