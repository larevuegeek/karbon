use crate::validation::constraints::{CollectionConstraint, ConstraintResult, ConstraintViolation};

/// Validates that a collection is not empty.
pub struct NotEmpty {
    pub message: String,
}

impl Default for NotEmpty {
    fn default() -> Self {
        Self {
            message: "This collection should not be empty.".to_string(),
        }
    }
}

impl NotEmpty {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = message.into();
        self
    }
}

impl CollectionConstraint for NotEmpty {
    fn validate_slice<T>(&self, value: &[T]) -> ConstraintResult {
        if value.is_empty() {
            return Err(ConstraintViolation::new(self.name(), &self.message, "[]"));
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        "NotEmpty"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_not_empty() {
        let constraint = NotEmpty::new();
        assert!(constraint.validate_slice(&[1, 2, 3]).is_ok());
        assert!(constraint.validate_slice(&["a"]).is_ok());
    }

    #[test]
    fn test_empty() {
        let constraint = NotEmpty::new();
        assert!(constraint.validate_slice::<i32>(&[]).is_err());
    }

    #[test]
    fn test_custom_message() {
        let constraint = NotEmpty::new().with_message("Ajoutez au moins un élément.");
        let err = constraint.validate_slice::<i32>(&[]).unwrap_err();
        assert_eq!(err.message, "Ajoutez au moins un élément.");
    }
}
