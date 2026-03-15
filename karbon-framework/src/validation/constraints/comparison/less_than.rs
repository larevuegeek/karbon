use crate::validation::constraints::{ConstraintResult, ConstraintViolation, NumericConstraint};

/// Validates that a value is less than a given number.
///
/// Equivalent to Symfony's `LessThan` constraint.
pub struct LessThan {
    pub value: f64,
    pub message: String,
    pub or_equal: bool,
}

impl LessThan {
    pub fn new(value: f64) -> Self {
        Self {
            value,
            message: "This value should be less than {{ compared_value }}.".to_string(),
            or_equal: false,
        }
    }

    /// LessThanOrEqual variant
    pub fn or_equal(value: f64) -> Self {
        Self {
            value,
            message: "This value should be less than or equal to {{ compared_value }}.".to_string(),
            or_equal: true,
        }
    }

    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = message.into();
        self
    }
}

impl NumericConstraint for LessThan {
    fn validate_f64(&self, value: f64) -> ConstraintResult {
        let valid = if self.or_equal {
            value <= self.value
        } else {
            value < self.value
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
        "LessThan"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_less_than() {
        let constraint = LessThan::new(10.0);
        assert!(constraint.validate_f64(9.0).is_ok());
        assert!(constraint.validate_f64(0.0).is_ok());
        assert!(constraint.validate_f64(10.0).is_err()); // not strictly less
        assert!(constraint.validate_f64(11.0).is_err());
    }

    #[test]
    fn test_less_than_or_equal() {
        let constraint = LessThan::or_equal(10.0);
        assert!(constraint.validate_f64(10.0).is_ok());
        assert!(constraint.validate_f64(9.0).is_ok());
        assert!(constraint.validate_f64(11.0).is_err());
    }

    #[test]
    fn test_with_negative_values() {
        let constraint = LessThan::new(0.0);
        assert!(constraint.validate_f64(-1.0).is_ok());
        assert!(constraint.validate_f64(-100.0).is_ok());
        assert!(constraint.validate_f64(0.0).is_err());
        assert!(constraint.validate_f64(1.0).is_err());
    }
}
