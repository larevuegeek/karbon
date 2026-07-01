//! Form builder (à la Symfony Forms).
//!
//! Define a form declaratively, bind submitted data, validate (reusing the
//! validation constraints), and render HTML — including errors and an optional
//! CSRF hidden field.
//!
//! ```ignore
//! use karbon::form::{Form, Field};
//! use karbon::validation::constraints::string::Length;
//!
//! let mut form = Form::new()
//!     .add(Field::text("title", "Titre").required().constraint(Length::new().min(3)))
//!     .add(Field::textarea("body", "Contenu").required())
//!     .add(Field::checkbox("published", "Publié"))
//!     .csrf(csrf_token);
//!
//! if form.handle(&submitted) {        // bind + validate
//!     let title = form.value("title").unwrap_or_default();
//!     // persist…
//! } else {
//!     let html = form.render();        // re-render with errors
//! }
//! ```

mod field;

pub use field::{Field, FieldKind};

use std::collections::HashMap;

use crate::validation::ValidationErrors;
use field::esc;

/// A collection of [`Field`]s with binding, validation and HTML rendering.
#[derive(Default)]
pub struct Form {
    fields: Vec<Field>,
    csrf: Option<(String, String)>,
    submitted: bool,
}

impl Form {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a field.
    #[allow(clippy::should_implement_trait)] // builder-style `add`, not `std::ops::Add`
    pub fn add(mut self, field: Field) -> Self {
        self.fields.push(field);
        self
    }

    /// Add a CSRF hidden field named `_csrf` with the given token. CSRF itself
    /// is enforced by the middleware; this only embeds the token in the form.
    pub fn csrf(mut self, token: impl Into<String>) -> Self {
        self.csrf = Some(("_csrf".to_string(), token.into()));
        self
    }

    /// Bind submitted data to fields, validate, and return whether the form is valid.
    pub fn handle(&mut self, data: &HashMap<String, String>) -> bool {
        self.submitted = true;
        let mut valid = true;
        for field in &mut self.fields {
            let value = match &field.kind {
                // Unchecked checkboxes are absent from the payload.
                FieldKind::Checkbox => {
                    if data.contains_key(&field.name) {
                        "1".to_string()
                    } else {
                        "0".to_string()
                    }
                }
                _ => data.get(&field.name).cloned().unwrap_or_default(),
            };
            field.value = value;
            if !field.validate() {
                valid = false;
            }
        }
        valid
    }

    /// Pre-fill field values (e.g. when editing an existing record).
    pub fn fill(mut self, values: &HashMap<String, String>) -> Self {
        for field in &mut self.fields {
            if let Some(v) = values.get(&field.name) {
                field.value = v.clone();
            }
        }
        self
    }

    pub fn is_valid(&self) -> bool {
        self.submitted && self.fields.iter().all(|f| f.errors.is_empty())
    }

    pub fn was_submitted(&self) -> bool {
        self.submitted
    }

    /// Current value of a field.
    pub fn value(&self, name: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|f| f.name == name)
            .map(|f| f.value.as_str())
    }

    /// All field values keyed by name.
    pub fn values(&self) -> HashMap<String, String> {
        self.fields
            .iter()
            .map(|f| (f.name.clone(), f.value.clone()))
            .collect()
    }

    /// Validation errors keyed by field.
    pub fn errors(&self) -> ValidationErrors {
        let mut errors = ValidationErrors::new();
        for field in &self.fields {
            for e in &field.errors {
                errors.add(&field.name, e.clone());
            }
        }
        errors
    }

    /// Render the inner form fields (labels + controls + errors + CSRF hidden).
    /// Wrap this in your own `<form>` element.
    pub fn render(&self) -> String {
        let mut out = String::new();
        if let Some((name, token)) = &self.csrf {
            out.push_str(&format!(
                "<input type=\"hidden\" name=\"{}\" value=\"{}\">",
                esc(name),
                esc(token)
            ));
        }
        for field in &self.fields {
            out.push_str(&field.render());
        }
        out
    }

    /// Render a complete `<form>` with the fields and a submit button.
    pub fn render_form(&self, action: &str, submit_label: &str) -> String {
        format!(
            "<form method=\"post\" action=\"{}\">{}<button type=\"submit\">{}</button></form>",
            esc(action),
            self.render(),
            esc(submit_label)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validation::constraints::string::Length;

    fn data(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn valid_submission() {
        let mut form = Form::new()
            .add(
                Field::text("title", "Titre")
                    .required()
                    .constraint(Length::new().min(3)),
            )
            .add(Field::checkbox("published", "Publié"));
        let ok = form.handle(&data(&[("title", "Hello"), ("published", "1")]));
        assert!(ok);
        assert!(form.is_valid());
        assert_eq!(form.value("title"), Some("Hello"));
        assert_eq!(form.value("published"), Some("1"));
    }

    #[test]
    fn required_and_constraint_errors() {
        let mut form = Form::new().add(
            Field::text("title", "Titre")
                .required()
                .constraint(Length::new().min(3)),
        );
        let ok = form.handle(&data(&[("title", "x")]));
        assert!(!ok);
        assert!(!form.is_valid());
        let errors = form.errors();
        assert!(errors.fields().contains_key("title"));
    }

    #[test]
    fn unchecked_checkbox_is_zero() {
        let mut form = Form::new().add(Field::checkbox("published", "Publié"));
        form.handle(&data(&[])); // checkbox absent
        assert_eq!(form.value("published"), Some("0"));
    }

    #[test]
    fn render_escapes_and_includes_csrf() {
        let form = Form::new().add(Field::text("name", "Name")).csrf("tok<en");
        let html = form.render();
        assert!(html.contains("name=\"_csrf\""));
        assert!(html.contains("tok&lt;en"));
        assert!(html.contains("name=\"name\""));
    }

    #[test]
    fn select_marks_selected_option() {
        let form = Form::new().add(
            Field::select("role", "Role", vec![("admin", "Admin"), ("user", "User")]).value("user"),
        );
        let html = form.render();
        assert!(html.contains("<option value=\"user\" selected>User</option>"));
    }
}
