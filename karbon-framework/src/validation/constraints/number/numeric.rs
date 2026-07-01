use crate::validation::constraints::{Constraint, ConstraintResult, ConstraintViolation};

/// Validates that a value is numeric (integer or float).
pub struct Numeric {
    pub message: String,
    pub allow_float: bool,
}

impl Default for Numeric {
    fn default() -> Self {
        Self {
            message: "This value should be a valid number.".to_string(),
            allow_float: true,
        }
    }
}

impl Numeric {
    pub fn new() -> Self {
        Self::default()
    }

    /// Only allow integer values
    pub fn integer_only() -> Self {
        Self {
            allow_float: false,
            ..Self::default()
        }
    }

    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = message.into();
        self
    }
}

impl Constraint for Numeric {
    fn validate(&self, value: &str) -> ConstraintResult {
        let valid = if self.allow_float {
            value.parse::<f64>().is_ok()
        } else {
            value.parse::<i64>().is_ok()
        };

        if !valid {
            return Err(ConstraintViolation::new(self.name(), &self.message, value));
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        "Numeric"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_numbers() {
        let constraint = Numeric::new();
        assert!(constraint.validate("42").is_ok());
        assert!(constraint.validate("-10").is_ok());
        assert!(constraint.validate("3.14").is_ok());
        assert!(constraint.validate("0").is_ok());
        assert!(constraint.validate("-0.5").is_ok());
    }

    #[test]
    fn test_invalid_numbers() {
        let constraint = Numeric::new();
        assert!(constraint.validate("").is_err());
        assert!(constraint.validate("abc").is_err());
        assert!(constraint.validate("12abc").is_err());
        assert!(constraint.validate("12.34.56").is_err());
    }

    #[test]
    fn test_integer_only() {
        let constraint = Numeric::integer_only();
        assert!(constraint.validate("42").is_ok());
        assert!(constraint.validate("-10").is_ok());
        assert!(constraint.validate("3.14").is_err());
        assert!(constraint.validate("0.5").is_err());
    }
}
