//! Template engine backed by [minijinja](https://docs.rs/minijinja) (Jinja2/Twig-like).

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

use minijinja::Environment;
use serde_json::Value;

use super::renderer::Renderer;
use crate::error::{AppError, AppResult};

/// Minijinja-backed template engine. Loads `**/*.{html,txt,xml,j2,jinja}` from a
/// directory at startup and implements [`Renderer`].
#[derive(Clone)]
pub struct MinijinjaEngine {
    env: Arc<Environment<'static>>,
    names: Arc<BTreeSet<String>>,
}

impl MinijinjaEngine {
    /// Load all templates from `template_dir` (recursively).
    pub fn new(template_dir: &str) -> AppResult<Self> {
        let mut env = Environment::new();
        let mut names = BTreeSet::new();
        let base = Path::new(template_dir);
        load_dir(&mut env, &mut names, base, base)?;
        tracing::info!(
            "Minijinja engine loaded {} templates from '{}'",
            names.len(),
            template_dir
        );
        Ok(Self {
            env: Arc::new(env),
            names: Arc::new(names),
        })
    }

    /// An engine with no templates loaded.
    pub fn empty() -> Self {
        Self {
            env: Arc::new(Environment::new()),
            names: Arc::new(BTreeSet::new()),
        }
    }
}

fn load_dir(
    env: &mut Environment<'static>,
    names: &mut BTreeSet<String>,
    base: &Path,
    dir: &Path,
) -> AppResult<()> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            return Err(AppError::Internal(format!(
                "Failed to read templates from '{}': {e}",
                dir.display()
            )));
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            load_dir(env, names, base, &path)?;
            continue;
        }
        let is_template = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| matches!(e, "html" | "txt" | "xml" | "j2" | "jinja"))
            .unwrap_or(false);
        if !is_template {
            continue;
        }

        let rel = path
            .strip_prefix(base)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        let source = std::fs::read_to_string(&path)
            .map_err(|e| AppError::Internal(format!("Cannot read template {rel}: {e}")))?;

        env.add_template_owned(rel.clone(), source)
            .map_err(|e| AppError::Internal(format!("Invalid template {rel}: {e}")))?;
        names.insert(rel);
    }
    Ok(())
}

impl Renderer for MinijinjaEngine {
    fn render(&self, name: &str, context: &Value) -> AppResult<String> {
        let tmpl = self
            .env
            .get_template(name)
            .map_err(|e| AppError::Internal(format!("Template '{name}' not found: {e}")))?;
        tmpl.render(context)
            .map_err(|e| AppError::Internal(format!("Template render error '{name}': {e}")))
    }

    fn render_str(&self, source: &str, context: &Value) -> AppResult<String> {
        self.env
            .render_str(source, context)
            .map_err(|e| AppError::Internal(format!("Template string render error: {e}")))
    }

    fn has_template(&self, name: &str) -> bool {
        self.names.contains(name)
    }

    fn template_names(&self) -> Vec<String> {
        self.names.iter().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn renders_inline_string() {
        let engine = MinijinjaEngine::empty();
        let out = engine
            .render_str("Hello {{ name }}!", &json!({ "name": "David" }))
            .unwrap();
        assert_eq!(out, "Hello David!");
    }

    #[test]
    fn empty_engine_has_no_templates() {
        let engine = MinijinjaEngine::empty();
        assert!(!engine.has_template("x.html"));
        assert!(engine.template_names().is_empty());
    }

    #[test]
    fn loop_and_condition() {
        let engine = MinijinjaEngine::empty();
        let out = engine
            .render_str(
                "{% for i in items %}{{ i }}{% if not loop.last %},{% endif %}{% endfor %}",
                &json!({ "items": [1, 2, 3] }),
            )
            .unwrap();
        assert_eq!(out, "1,2,3");
    }
}
