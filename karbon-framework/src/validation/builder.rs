//! Composable validator (à la Symfony Validator).
//!
//! Aggregates **all** violations across fields (instead of failing on the first),
//! supports nested object validation, ad-hoc checks, custom constraints and
//! validation groups. Builds on the existing [`Constraint`] / [`NumericConstraint`]
//! traits.
//!
//! ```ignore
//! use karbon::validation::{Validator, constraints::string::{Length, NotBlank}};
//!
//! let mut v = Validator::new();
//! v.field("name", &input.name, &[&NotBlank::new(), &Length::new().min(2).max(50)]);
//! v.check("age", input.age >= 18, "Vous devez être majeur");
//! v.into_result()?; // -> AppError::Validation listing every error, or Ok
//! ```

use std::collections::BTreeMap;
use std::collections::HashSet;

use super::constraints::{Constraint, NumericConstraint};
use crate::error::AppError;

const DEFAULT_GROUP: &str = "Default";

/// Field-keyed collection of validation messages.
#[derive(Debug, Default, Clone)]
pub struct ValidationErrors {
    fields: BTreeMap<String, Vec<String>>,
}

impl ValidationErrors {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a message under a field.
    pub fn add(&mut self, field: impl Into<String>, message: impl Into<String>) {
        self.fields
            .entry(field.into())
            .or_default()
            .push(message.into());
    }

    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    pub fn is_valid(&self) -> bool {
        self.is_empty()
    }

    /// Field → messages map.
    pub fn fields(&self) -> &BTreeMap<String, Vec<String>> {
        &self.fields
    }

    /// Merge another set of errors, prefixing every field with `prefix.`
    /// (used for nested object validation).
    pub fn merge_prefixed(&mut self, prefix: &str, other: ValidationErrors) {
        for (field, messages) in other.fields {
            let key = format!("{prefix}.{field}");
            self.fields.entry(key).or_default().extend(messages);
        }
    }

    /// Flatten to a single human-readable string (`field: msg, field: msg`).
    pub fn to_message(&self) -> String {
        self.fields
            .iter()
            .flat_map(|(field, msgs)| msgs.iter().map(move |m| format!("{field}: {m}")))
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// `Ok(())` if empty, otherwise `AppError::Validation`.
    pub fn into_result(self) -> Result<(), AppError> {
        if self.is_empty() {
            Ok(())
        } else {
            Err(AppError::Validation(self.to_message()))
        }
    }
}

/// Fluent validator collecting violations across many fields.
pub struct Validator {
    errors: ValidationErrors,
    active_groups: HashSet<String>,
}

impl Default for Validator {
    fn default() -> Self {
        Self::new()
    }
}

impl Validator {
    /// Validator running the `Default` group.
    pub fn new() -> Self {
        Self {
            errors: ValidationErrors::new(),
            active_groups: HashSet::from([DEFAULT_GROUP.to_string()]),
        }
    }

    /// Validator running only the given groups (ungrouped fields are skipped
    /// unless `"Default"` is included).
    pub fn for_groups<I, S>(groups: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            errors: ValidationErrors::new(),
            active_groups: groups.into_iter().map(Into::into).collect(),
        }
    }

    fn group_active(&self, group: &str) -> bool {
        self.active_groups.contains(group)
    }

    /// Validate a string field against constraints (group `Default`).
    pub fn field(&mut self, name: &str, value: &str, constraints: &[&dyn Constraint]) -> &mut Self {
        self.field_in(DEFAULT_GROUP, name, value, constraints)
    }

    /// Validate a string field in a specific group.
    pub fn field_in(
        &mut self,
        group: &str,
        name: &str,
        value: &str,
        constraints: &[&dyn Constraint],
    ) -> &mut Self {
        if self.group_active(group) {
            for c in constraints {
                if let Err(v) = c.validate(value) {
                    self.errors.add(name, v.message);
                }
            }
        }
        self
    }

    /// Validate a numeric field against numeric constraints (group `Default`).
    pub fn field_num(
        &mut self,
        name: &str,
        value: f64,
        constraints: &[&dyn NumericConstraint],
    ) -> &mut Self {
        if self.group_active(DEFAULT_GROUP) {
            for c in constraints {
                if let Err(v) = c.validate_f64(value) {
                    self.errors.add(name, v.message);
                }
            }
        }
        self
    }

    /// Add an error to `name` unless `condition` holds (custom/ad-hoc rule).
    pub fn check(&mut self, name: &str, condition: bool, message: impl Into<String>) -> &mut Self {
        if !condition {
            self.errors.add(name, message);
        }
        self
    }

    /// Merge a nested object's errors under `name.` (e.g. `address.city`).
    pub fn nested(&mut self, name: &str, errors: ValidationErrors) -> &mut Self {
        if !errors.is_empty() {
            self.errors.merge_prefixed(name, errors);
        }
        self
    }

    /// Current accumulated errors.
    pub fn errors(&self) -> &ValidationErrors {
        &self.errors
    }

    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    /// Consume and turn into a `Result` (`AppError::Validation` if any error).
    pub fn into_result(self) -> Result<(), AppError> {
        self.errors.into_result()
    }

    /// Consume and return the collected errors.
    pub fn into_errors(self) -> ValidationErrors {
        self.errors
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validation::constraints::string::{Length, NotBlank};

    #[test]
    fn collects_all_field_errors() {
        let mut v = Validator::new();
        v.field("name", "", &[&NotBlank::new(), &Length::new().min(2)]);
        v.field("bio", "ok", &[&Length::new().min(2)]);
        let errors = v.into_errors();
        // name fails NotBlank + Length(min 2); bio passes
        assert!(!errors.is_empty());
        assert_eq!(errors.fields().get("name").map(|m| m.len()), Some(2));
        assert!(!errors.fields().contains_key("bio"));
    }

    #[test]
    fn check_adds_custom_error() {
        let mut v = Validator::new();
        v.check("age", 15 >= 18, "Vous devez être majeur");
        assert!(!v.is_valid());
        assert!(v.into_result().is_err());
    }

    #[test]
    fn valid_input_is_ok() {
        let mut v = Validator::new();
        v.field(
            "name",
            "David",
            &[&NotBlank::new(), &Length::new().min(2).max(50)],
        );
        assert!(v.is_valid());
        assert!(v.into_result().is_ok());
    }

    #[test]
    fn nested_errors_are_prefixed() {
        let mut child = ValidationErrors::new();
        child.add("city", "required");
        let mut v = Validator::new();
        v.nested("address", child);
        let errors = v.into_errors();
        assert!(errors.fields().contains_key("address.city"));
    }

    #[test]
    fn groups_filter_validation() {
        // Only the "create" group is active → the Default field is skipped.
        let mut v = Validator::for_groups(["create"]);
        v.field("default_field", "", &[&NotBlank::new()]); // Default group → skipped
        v.field_in("create", "create_field", "", &[&NotBlank::new()]); // active → fails
        let errors = v.into_errors();
        assert!(!errors.fields().contains_key("default_field"));
        assert!(errors.fields().contains_key("create_field"));
    }
}
