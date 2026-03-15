use crate::validation::constraints::{ConstraintResult, ConstraintViolation};
use std::collections::HashSet;
use std::hash::Hash;

/// Validates that all elements in a collection are unique.
///
/// Equivalent to Symfony's `Unique` constraint.
pub struct Unique {
    pub message: String,
}

impl Default for Unique {
    fn default() -> Self {
        Self {
            message: "This collection should contain only unique elements.".to_string(),
        }
    }
}

impl Unique {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = message.into();
        self
    }

    pub fn validate_slice<T: Eq + Hash>(&self, value: &[T]) -> ConstraintResult {
        let mut seen = HashSet::new();
        for item in value {
            if !seen.insert(item) {
                return Err(ConstraintViolation::new(
                    "Unique",
                    &self.message,
                    format!("count: {}", value.len()),
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unique_elements() {
        let constraint = Unique::new();
        assert!(constraint.validate_slice(&[1, 2, 3, 4]).is_ok());
        assert!(constraint.validate_slice(&["a", "b", "c"]).is_ok());
        assert!(constraint.validate_slice::<i32>(&[]).is_ok());
    }

    #[test]
    fn test_duplicate_elements() {
        let constraint = Unique::new();
        assert!(constraint.validate_slice(&[1, 2, 3, 2]).is_err());
        assert!(constraint.validate_slice(&["a", "b", "a"]).is_err());
        assert!(constraint.validate_slice(&[1, 1]).is_err());
    }

    #[test]
    fn test_single_element() {
        let constraint = Unique::new();
        assert!(constraint.validate_slice(&[42]).is_ok());
    }
}
