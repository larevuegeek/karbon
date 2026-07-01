use crate::error::AppResult;
use serde_json::Value;

/// Backend-agnostic template rendering interface.
///
/// Implemented by the Tera-based [`super::TemplateEngine`] (feature `templates`)
/// and the minijinja-based [`super::MinijinjaEngine`] (feature `minijinja`).
/// A future in-house engine will implement the same trait, so application code
/// can stay backend-independent (`Arc<dyn Renderer>`).
///
/// The context is a [`serde_json::Value`] (typically a JSON object), so any
/// `Serialize` type works via `serde_json::to_value`.
pub trait Renderer: Send + Sync {
    /// Render a named template with the given context.
    fn render(&self, name: &str, context: &Value) -> AppResult<String>;

    /// Render a template from a raw source string.
    fn render_str(&self, source: &str, context: &Value) -> AppResult<String>;

    /// Whether a template with this name is loaded.
    fn has_template(&self, name: &str) -> bool;

    /// Names of all loaded templates.
    fn template_names(&self) -> Vec<String>;
}
