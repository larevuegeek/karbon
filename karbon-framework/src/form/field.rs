use crate::validation::Constraint;

/// The HTML input kind of a [`Field`].
pub enum FieldKind {
    Text,
    Email,
    Password,
    Number,
    Tel,
    Url,
    Date,
    Hidden,
    Textarea,
    Checkbox,
    /// `<select>` with `(value, label)` options.
    Select(Vec<(String, String)>),
}

impl FieldKind {
    fn input_type(&self) -> &'static str {
        match self {
            FieldKind::Text => "text",
            FieldKind::Email => "email",
            FieldKind::Password => "password",
            FieldKind::Number => "number",
            FieldKind::Tel => "tel",
            FieldKind::Url => "url",
            FieldKind::Date => "date",
            FieldKind::Hidden => "hidden",
            _ => "text",
        }
    }
}

/// A single form field: kind, label, value, constraints and (after handling)
/// its validation errors.
pub struct Field {
    pub(crate) name: String,
    pub(crate) label: String,
    pub(crate) kind: FieldKind,
    pub(crate) required: bool,
    pub(crate) value: String,
    pub(crate) placeholder: Option<String>,
    pub(crate) help: Option<String>,
    pub(crate) constraints: Vec<Box<dyn Constraint>>,
    pub(crate) errors: Vec<String>,
}

impl Field {
    fn base(name: &str, label: &str, kind: FieldKind) -> Self {
        Self {
            name: name.to_string(),
            label: label.to_string(),
            kind,
            required: false,
            value: String::new(),
            placeholder: None,
            help: None,
            constraints: Vec::new(),
            errors: Vec::new(),
        }
    }

    pub fn text(name: &str, label: &str) -> Self {
        Self::base(name, label, FieldKind::Text)
    }
    pub fn email(name: &str, label: &str) -> Self {
        Self::base(name, label, FieldKind::Email)
    }
    pub fn password(name: &str, label: &str) -> Self {
        Self::base(name, label, FieldKind::Password)
    }
    pub fn number(name: &str, label: &str) -> Self {
        Self::base(name, label, FieldKind::Number)
    }
    pub fn tel(name: &str, label: &str) -> Self {
        Self::base(name, label, FieldKind::Tel)
    }
    pub fn url(name: &str, label: &str) -> Self {
        Self::base(name, label, FieldKind::Url)
    }
    pub fn date(name: &str, label: &str) -> Self {
        Self::base(name, label, FieldKind::Date)
    }
    pub fn hidden(name: &str, value: &str) -> Self {
        let mut f = Self::base(name, "", FieldKind::Hidden);
        f.value = value.to_string();
        f
    }
    pub fn textarea(name: &str, label: &str) -> Self {
        Self::base(name, label, FieldKind::Textarea)
    }
    pub fn checkbox(name: &str, label: &str) -> Self {
        Self::base(name, label, FieldKind::Checkbox)
    }
    pub fn select<K: Into<String>, V: Into<String>>(
        name: &str,
        label: &str,
        options: Vec<(K, V)>,
    ) -> Self {
        let opts = options
            .into_iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect();
        Self::base(name, label, FieldKind::Select(opts))
    }

    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }
    pub fn value(mut self, value: impl Into<String>) -> Self {
        self.value = value.into();
        self
    }
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }
    pub fn help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }
    /// Attach a validation [`Constraint`] (from `karbon::validation::constraints`).
    pub fn constraint<C: Constraint + 'static>(mut self, constraint: C) -> Self {
        self.constraints.push(Box::new(constraint));
        self
    }

    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn current_value(&self) -> &str {
        &self.value
    }
    pub fn field_errors(&self) -> &[String] {
        &self.errors
    }

    /// Validate the field's current value, populating `errors`. Returns true if valid.
    pub(crate) fn validate(&mut self) -> bool {
        self.errors.clear();

        if self.required && self.value.trim().is_empty() {
            self.errors.push("Ce champ est obligatoire".to_string());
            return false;
        }

        // Skip constraint checks on empty optional fields.
        if self.value.is_empty() {
            return true;
        }

        for c in &self.constraints {
            if let Err(v) = c.validate(&self.value) {
                self.errors.push(v.message);
            }
        }
        self.errors.is_empty()
    }

    /// Render the field's HTML (label + control + errors).
    pub(crate) fn render(&self) -> String {
        if matches!(self.kind, FieldKind::Hidden) {
            return format!(
                "<input type=\"hidden\" name=\"{}\" value=\"{}\">",
                esc(&self.name),
                esc(&self.value)
            );
        }

        let mut out = String::new();
        let invalid = if self.errors.is_empty() {
            ""
        } else {
            " has-error"
        };
        out.push_str(&format!("<div class=\"form-field{invalid}\">"));

        if !matches!(self.kind, FieldKind::Checkbox) {
            out.push_str(&format!(
                "<label for=\"{}\">{}{}</label>",
                esc(&self.name),
                esc(&self.label),
                if self.required { " *" } else { "" }
            ));
        }

        out.push_str(&self.render_control());

        if matches!(self.kind, FieldKind::Checkbox) {
            out.push_str(&format!(
                " <label for=\"{}\">{}</label>",
                esc(&self.name),
                esc(&self.label)
            ));
        }

        if let Some(help) = &self.help {
            out.push_str(&format!("<small class=\"form-help\">{}</small>", esc(help)));
        }
        for e in &self.errors {
            out.push_str(&format!("<div class=\"form-error\">{}</div>", esc(e)));
        }

        out.push_str("</div>");
        out
    }

    fn render_control(&self) -> String {
        let req = if self.required { " required" } else { "" };
        let placeholder = self
            .placeholder
            .as_ref()
            .map(|p| format!(" placeholder=\"{}\"", esc(p)))
            .unwrap_or_default();

        match &self.kind {
            FieldKind::Textarea => format!(
                "<textarea id=\"{}\" name=\"{}\"{}{}>{}</textarea>",
                esc(&self.name),
                esc(&self.name),
                req,
                placeholder,
                esc(&self.value)
            ),
            FieldKind::Checkbox => {
                let checked = if is_truthy(&self.value) {
                    " checked"
                } else {
                    ""
                };
                format!(
                    "<input type=\"checkbox\" id=\"{}\" name=\"{}\" value=\"1\"{}>",
                    esc(&self.name),
                    esc(&self.name),
                    checked
                )
            }
            FieldKind::Select(options) => {
                let mut s = format!(
                    "<select id=\"{}\" name=\"{}\"{}>",
                    esc(&self.name),
                    esc(&self.name),
                    req
                );
                for (val, label) in options {
                    let selected = if *val == self.value { " selected" } else { "" };
                    s.push_str(&format!(
                        "<option value=\"{}\"{}>{}</option>",
                        esc(val),
                        selected,
                        esc(label)
                    ));
                }
                s.push_str("</select>");
                s
            }
            kind => format!(
                "<input type=\"{}\" id=\"{}\" name=\"{}\" value=\"{}\"{}{}>",
                kind.input_type(),
                esc(&self.name),
                esc(&self.name),
                esc(&self.value),
                req,
                placeholder
            ),
        }
    }
}

fn is_truthy(v: &str) -> bool {
    matches!(v, "1" | "on" | "true" | "yes")
}

/// HTML-escape attribute/text content.
pub(crate) fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}
