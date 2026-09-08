//! Rules that extend the `#[validate(...)]` vocabulary of the `validator` crate.
//!
//! Wired up by the [`validated`](karbon_macros::validated) attribute macro — an
//! application writes `#[validate(accepted(message = "…"))]`, never these
//! functions directly.

use validator::ValidationError;

/// Rejects a `bool` that is not `true` — for "I accept the terms" checkboxes.
///
/// `validator` has no built-in rule for this: `required` only checks that an
/// `Option` is `Some`, so an explicitly unchecked box would pass.
///
/// On an `Option<bool>` field the `validated` macro pairs this with `required`,
/// because `validator` skips custom validators on a `None` value.
pub fn accepted(value: &bool) -> Result<(), ValidationError> {
    if *value {
        Ok(())
    } else {
        Err(ValidationError::new("accepted").with_message("This box must be checked.".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_true_passes() {
        assert!(accepted(&true).is_ok());
        assert!(accepted(&false).is_err());
    }
}
