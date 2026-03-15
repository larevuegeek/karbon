use crate::validation::constraints::{CollectionConstraint, ConstraintResult, ConstraintViolation};

/// Validates that a collection has a count within a given range.
///
/// Equivalent to Symfony's `Count` constraint.
pub struct Count {
    pub min: Option<usize>,
    pub max: Option<usize>,
    pub exact: Option<usize>,
    pub min_message: String,
    pub max_message: String,
    pub exact_message: String,
}

impl Default for Count {
    fn default() -> Self {
        Self {
            min: None,
            max: None,
            exact: None,
            min_message: "This collection should contain {{ limit }} elements or more.".to_string(),
            max_message: "This collection should contain {{ limit }} elements or less.".to_string(),
            exact_message: "This collection should contain exactly {{ limit }} elements.".to_string(),
        }
    }
}

impl Count {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn min(mut self, min: usize) -> Self {
        self.min = Some(min);
        self
    }

    pub fn max(mut self, max: usize) -> Self {
        self.max = Some(max);
        self
    }

    pub fn exact(mut self, exact: usize) -> Self {
        self.exact = Some(exact);
        self
    }

    pub fn between(min: usize, max: usize) -> Self {
        Self {
            min: Some(min),
            max: Some(max),
            ..Self::default()
        }
    }

    fn format_message(template: &str, limit: usize) -> String {
        template.replace("{{ limit }}", &limit.to_string())
    }
}

impl CollectionConstraint for Count {
    fn validate_slice<T>(&self, value: &[T]) -> ConstraintResult {
        let len = value.len();

        if let Some(exact) = self.exact {
            if len != exact {
                return Err(ConstraintViolation::new(
                    self.name(),
                    Self::format_message(&self.exact_message, exact),
                    format!("count: {}", len),
                ));
            }
            return Ok(());
        }

        if let Some(min) = self.min {
            if len < min {
                return Err(ConstraintViolation::new(
                    self.name(),
                    Self::format_message(&self.min_message, min),
                    format!("count: {}", len),
                ));
            }
        }

        if let Some(max) = self.max {
            if len > max {
                return Err(ConstraintViolation::new(
                    self.name(),
                    Self::format_message(&self.max_message, max),
                    format!("count: {}", len),
                ));
            }
        }

        Ok(())
    }

    fn name(&self) -> &'static str {
        "Count"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_min_count() {
        let constraint = Count::new().min(2);
        assert!(constraint.validate_slice(&[1, 2]).is_ok());
        assert!(constraint.validate_slice(&[1, 2, 3]).is_ok());
        assert!(constraint.validate_slice(&[1]).is_err());
    }

    #[test]
    fn test_max_count() {
        let constraint = Count::new().max(3);
        assert!(constraint.validate_slice(&[1, 2, 3]).is_ok());
        assert!(constraint.validate_slice(&[1]).is_ok());
        assert!(constraint.validate_slice(&[1, 2, 3, 4]).is_err());
    }

    #[test]
    fn test_between_count() {
        let constraint = Count::between(2, 4);
        assert!(constraint.validate_slice(&[1, 2]).is_ok());
        assert!(constraint.validate_slice(&[1, 2, 3, 4]).is_ok());
        assert!(constraint.validate_slice(&[1]).is_err());
        assert!(constraint.validate_slice(&[1, 2, 3, 4, 5]).is_err());
    }

    #[test]
    fn test_exact_count() {
        let constraint = Count::new().exact(3);
        assert!(constraint.validate_slice(&[1, 2, 3]).is_ok());
        assert!(constraint.validate_slice(&[1, 2]).is_err());
        assert!(constraint.validate_slice(&[1, 2, 3, 4]).is_err());
    }
}
