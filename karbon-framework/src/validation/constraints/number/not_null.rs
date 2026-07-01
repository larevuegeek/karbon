use crate::validation::constraints::{Constraint, ConstraintResult, ConstraintViolation};

/// Validates that a value is not null/empty.
///
/// Equivalent to Symfony's `NotNull` constraint.
/// In Rust string context, this checks that the value is not empty.
pub struct NotNull {
    pub message: String,
}

impl Default for NotNull {
    fn default() -> Self {
        Self {
            message: "This value should not be null.".to_string(),
        }
    }
}

impl NotNull {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = message.into();
        self
    }
}

impl Constraint for NotNull {
    fn validate(&self, value: &str) -> ConstraintResult {
        if value.is_empty() {
            return Err(ConstraintViolation::new(self.name(), &self.message, value));
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        "NotNull"
    }
}

/// Validates that an `Option<T>` is `Some`.
#[allow(dead_code)]
pub fn validate_not_null<T>(value: &Option<T>, message: Option<&str>) -> ConstraintResult {
    if value.is_none() {
        return Err(ConstraintViolation::new(
            "NotNull",
            message.unwrap_or("This value should not be null."),
            "null",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_not_null_string() {
        let constraint = NotNull::new();
        assert!(constraint.validate("hello").is_ok());
        assert!(constraint.validate("0").is_ok());
        assert!(constraint.validate("").is_err());
    }

    #[test]
    fn test_not_null_option() {
        assert!(validate_not_null(&Some(42), None).is_ok());
        assert!(validate_not_null(&Some(""), None).is_ok());
        assert!(validate_not_null::<i32>(&None, None).is_err());
    }

    #[test]
    fn test_custom_message() {
        let constraint = NotNull::new().with_message("Ce champ est obligatoire.");
        let err = constraint.validate("").unwrap_err();
        assert_eq!(err.message, "Ce champ est obligatoire.");
    }

    #[test]
    fn test_option_custom_message() {
        let err = validate_not_null::<i32>(&None, Some("Valeur requise.")).unwrap_err();
        assert_eq!(err.message, "Valeur requise.");
    }
}
