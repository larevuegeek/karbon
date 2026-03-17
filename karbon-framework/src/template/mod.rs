//! # Template Engine
//!
//! Tera-based template engine with layout inheritance, custom filters,
//! and integrated email sending.
//!
//! ## Usage
//!
//! ```rust,ignore
//! // Render a template
//! let html = state.templates.render("emails/welcome.html", &ctx)?;
//!
//! // Send email with template
//! state.templates.send_mail(&mailer, "emails/welcome.html", &ctx, &opts)?;
//! ```
//!
//! ## Template syntax (Jinja2/Twig)
//!
//! ```html
//! {% extends "layouts/email.html" %}
//! {% block content %}
//!   <h1>Hello {{ username }}!</h1>
//!   <p>Date: {{ now | date_fr }}</p>
//! {% endblock %}
//! ```

mod engine;
mod filters;
mod mail;

pub use engine::TemplateEngine;
pub use mail::MailTemplateOptions;
