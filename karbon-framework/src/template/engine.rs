//! Template engine backed by Tera.

use std::sync::Arc;
use tera::{Context, Tera};
use crate::error::{AppError, AppResult};
use crate::mail::Mailer;
use super::filters;
use super::mail::MailTemplateOptions;

/// Template engine wrapping Tera with custom filters and helpers.
///
/// # Example
/// ```rust,ignore
/// let engine = TemplateEngine::new("./templates")?;
/// let mut ctx = engine.context();
/// ctx.insert("username", "david");
/// let html = engine.render("emails/welcome.html", &ctx)?;
/// ```
#[derive(Clone)]
pub struct TemplateEngine {
    tera: Arc<Tera>,
    template_dir: String,
}

impl TemplateEngine {
    /// Create a new template engine from a directory.
    ///
    /// Loads all `**/*.html`, `**/*.xml`, `**/*.txt` files recursively.
    /// Registers custom filters (date_fr, currency, truncate_text, etc.).
    pub fn new(template_dir: &str) -> AppResult<Self> {
        let glob = format!("{}/**/*", template_dir);

        let mut tera = Tera::new(&glob).map_err(|e| {
            AppError::Internal(format!(
                "Failed to load templates from '{}': {}",
                template_dir, e
            ))
        })?;

        // Register custom filters
        filters::register_all(&mut tera);

        let names: Vec<_> = tera.get_template_names().collect();
        tracing::info!(
            "Template engine loaded {} templates from '{}'",
            names.len(),
            template_dir
        );

        Ok(Self {
            tera: Arc::new(tera),
            template_dir: template_dir.to_string(),
        })
    }

    /// Create an empty template engine (no templates loaded).
    /// Useful as fallback when no template directory is configured.
    pub fn empty() -> Self {
        let mut tera = Tera::default();
        filters::register_all(&mut tera);
        Self {
            tera: Arc::new(tera),
            template_dir: String::new(),
        }
    }

    /// Create a new Tera context with common variables pre-set.
    pub fn context(&self) -> Context {
        let mut ctx = Context::new();
        let now = chrono::Utc::now();
        ctx.insert("year", &now.format("%Y").to_string());
        ctx.insert("now", &now.format("%Y-%m-%d %H:%M:%S").to_string());
        ctx
    }

    /// Create a context with site-level variables.
    pub fn site_context(&self, site_name: &str, site_url: &str) -> Context {
        let mut ctx = self.context();
        ctx.insert("site_name", site_name);
        ctx.insert("site_url", site_url);
        ctx
    }

    /// Render a template to a string.
    pub fn render(&self, template_name: &str, context: &Context) -> AppResult<String> {
        self.tera.render(template_name, context).map_err(|e| {
            AppError::Internal(format!("Template render error '{}': {}", template_name, e))
        })
    }

    /// Render a template from a raw string (not from file).
    pub fn render_str(&self, template: &str, context: &Context) -> AppResult<String> {
        Tera::one_off(template, context, false).map_err(|e| {
            AppError::Internal(format!("Template string render error: {}", e))
        })
    }

    /// Render and send an HTML email.
    ///
    /// # Example
    /// ```rust,ignore
    /// let mut ctx = engine.site_context("LaRevueGeek", "https://www.larevuegeek.com");
    /// ctx.insert("username", "david");
    /// engine.send_mail(&mailer, "emails/welcome.html", &ctx, &MailTemplateOptions {
    ///     to: "user@example.com".into(),
    ///     subject: "Bienvenue !".into(),
    ///     ..Default::default()
    /// }).await?;
    /// ```
    pub async fn send_mail(
        &self,
        mailer: &Mailer,
        template_name: &str,
        context: &Context,
        opts: &MailTemplateOptions,
    ) -> AppResult<()> {
        let html = self.render(template_name, context)?;

        mailer.send_html(&opts.to, &opts.subject, &html).await
    }

    /// Reload templates from disk (useful in development).
    pub fn reload(&mut self) -> AppResult<()> {
        let glob = format!("{}/**/*", self.template_dir);
        let mut tera = Tera::new(&glob).map_err(|e| {
            AppError::Internal(format!("Template reload failed: {}", e))
        })?;
        filters::register_all(&mut tera);
        self.tera = Arc::new(tera);
        tracing::info!("Templates reloaded from '{}'", self.template_dir);
        Ok(())
    }

    /// Check if a template exists.
    pub fn has_template(&self, name: &str) -> bool {
        self.tera.get_template_names().any(|n| n == name)
    }

    /// List all loaded template names.
    pub fn template_names(&self) -> Vec<String> {
        self.tera.get_template_names().map(|s| s.to_string()).collect()
    }

    /// Get the template directory path.
    pub fn template_dir(&self) -> &str {
        &self.template_dir
    }
}

impl std::fmt::Debug for TemplateEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TemplateEngine")
            .field("template_dir", &self.template_dir)
            .field("templates_count", &self.tera.get_template_names().count())
            .finish()
    }
}
