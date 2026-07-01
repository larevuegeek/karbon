use crate::validation::constraints::{ConstraintResult, ConstraintViolation, NumericConstraint};

/// Validates that a number is within a given range.
///
/// Equivalent to Symfony's `Range` constraint.
pub struct Range {
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub min_message: String,
    pub max_message: String,
}

impl Default for Range {
    fn default() -> Self {
        Self {
            min: None,
            max: None,
            min_message: "This value should be {{ limit }} or more.".to_string(),
            max_message: "This value should be {{ limit }} or less.".to_string(),
        }
    }
}

impl Range {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn min(mut self, min: f64) -> Self {
        self.min = Some(min);
        self
    }

    pub fn max(mut self, max: f64) -> Self {
        self.max = Some(max);
        self
    }

    pub fn between(min: f64, max: f64) -> Self {
        Self {
            min: Some(min),
            max: Some(max),
            ..Self::default()
        }
    }

    pub fn with_min_message(mut self, message: impl Into<String>) -> Self {
        self.min_message = message.into();
        self
    }

    pub fn with_max_message(mut self, message: impl Into<String>) -> Self {
        self.max_message = message.into();
        self
    }

    fn format_message(template: &str, limit: f64) -> String {
        template.replace("{{ limit }}", &limit.to_string())
    }
}

impl NumericConstraint for Range {
    fn validate_f64(&self, value: f64) -> ConstraintResult {
        if let Some(min) = self.min
            && value < min
        {
            return Err(ConstraintViolation::new(
                self.name(),
                Self::format_message(&self.min_message, min),
                value.to_string(),
            ));
        }

        if let Some(max) = self.max
            && value > max
        {
            return Err(ConstraintViolation::new(
                self.name(),
                Self::format_message(&self.max_message, max),
                value.to_string(),
            ));
        }

        Ok(())
    }

    fn name(&self) -> &'static str {
        "Range"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_within_range() {
        let constraint = Range::between(1.0, 100.0);
        assert!(constraint.validate_f64(1.0).is_ok());
        assert!(constraint.validate_f64(50.0).is_ok());
        assert!(constraint.validate_f64(100.0).is_ok());
    }

    #[test]
    fn test_out_of_range() {
        let constraint = Range::between(1.0, 100.0);
        assert!(constraint.validate_f64(0.0).is_err());
        assert!(constraint.validate_f64(101.0).is_err());
        assert!(constraint.validate_f64(-5.0).is_err());
    }

    #[test]
    fn test_min_only() {
        let constraint = Range::new().min(0.0);
        assert!(constraint.validate_f64(0.0).is_ok());
        assert!(constraint.validate_f64(1000.0).is_ok());
        assert!(constraint.validate_f64(-1.0).is_err());
    }

    #[test]
    fn test_max_only() {
        let constraint = Range::new().max(100.0);
        assert!(constraint.validate_f64(-1000.0).is_ok());
        assert!(constraint.validate_f64(100.0).is_ok());
        assert!(constraint.validate_f64(101.0).is_err());
    }

    #[test]
    fn test_float_values() {
        let constraint = Range::between(0.0, 1.0);
        assert!(constraint.validate_f64(0.5).is_ok());
        assert!(constraint.validate_f64(0.99).is_ok());
        assert!(constraint.validate_f64(1.01).is_err());
    }
}
