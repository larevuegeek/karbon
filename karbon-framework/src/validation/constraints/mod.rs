pub mod collection;
pub mod comparison;
pub mod date;
pub mod number;
pub mod security;
pub mod string;

/// Result type for constraint validation
pub type ConstraintResult = Result<(), ConstraintViolation>;

/// Represents a validation constraint violation
#[derive(Debug, Clone)]
pub struct ConstraintViolation {
    pub constraint: &'static str,
    pub message: String,
    pub invalid_value: String,
}

impl ConstraintViolation {
    pub fn new(constraint: &'static str, message: impl Into<String>, invalid_value: impl Into<String>) -> Self {
        Self {
            constraint,
            message: message.into(),
            invalid_value: invalid_value.into(),
        }
    }
}

impl std::fmt::Display for ConstraintViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.constraint, self.message)
    }
}

impl std::error::Error for ConstraintViolation {}

/// Trait for all validation constraints operating on string values
pub trait Constraint {
    fn validate(&self, value: &str) -> ConstraintResult;
    fn name(&self) -> &'static str;
}

/// Trait for validation constraints operating on numeric values
pub trait NumericConstraint {
    fn validate_f64(&self, value: f64) -> ConstraintResult;
    fn name(&self) -> &'static str;
}

/// Trait for validation constraints operating on collections
pub trait CollectionConstraint {
    fn validate_slice<T>(&self, value: &[T]) -> ConstraintResult;
    fn name(&self) -> &'static str;
}

// Re-export all constraints
pub use collection::*;
pub use comparison::*;
pub use date::*;
pub use number::*;
pub use security::*;
pub use string::*;
