//! Template engine backed by Tera.

use super::filters;
use super::renderer::Renderer;
use crate::error::{AppError, AppResult};
use crate::mail::Mailer;
use serde_json::Value;
use std::sync::Arc;
use tera::{Context, Tera};

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
        Tera::one_off(template, context, false)
            .map_err(|e| AppError::Internal(format!("Template string render error: {}", e)))
    }

    /// Render and send an email (HTML + auto plain text fallback).
    ///
    /// If a `.txt` version of the template exists (e.g. `emails/welcome.txt`
    /// alongside `emails/welcome.html`), it is rendered and sent as the
    /// plain text part of a multipart email.
    ///
    /// # Example
    /// ```rust,ignore
    /// let mut ctx = engine.site_context("LaRevueGeek", "https://www.larevuegeek.com");
    /// ctx.insert("username", "david");
    /// engine.send_mail(&mailer, "to@mail.com", "Bienvenue !", "emails/welcome.html", &ctx).await?;
    /// ```
    pub async fn send_mail(
        &self,
        mailer: &Mailer,
        to: &str,
        subject: &str,
        template_name: &str,
        context: &Context,
    ) -> AppResult<()> {
        let html = self.render(template_name, context)?;

        // Auto-detect .txt counterpart for multipart
        let txt_name = template_name.replace(".html", ".txt");
        let has_txt = self.tera.get_template_names().any(|n| n == txt_name);

        if has_txt {
            let text = self.render(&txt_name, context)?;
            mailer
                .compose()
                .to(to)?
                .subject(subject)
                .text_and_html(&text, &html)
                .send()
                .await
        } else {
            mailer.send_html(to, subject, &html).await
        }
    }

    /// Same as `send_mail` but with a custom `from` address.
    pub async fn send_mail_from(
        &self,
        mailer: &Mailer,
        from: &str,
        to: &str,
        subject: &str,
        template_name: &str,
        context: &Context,
    ) -> AppResult<()> {
        let html = self.render(template_name, context)?;

        let txt_name = template_name.replace(".html", ".txt");
        let has_txt = self.tera.get_template_names().any(|n| n == txt_name);

        let builder = mailer.compose().from(from).to(to)?.subject(subject);

        if has_txt {
            let text = self.render(&txt_name, context)?;
            builder.text_and_html(&text, &html).send().await
        } else {
            builder.html(&html).send().await
        }
    }

    /// Reload templates from disk (useful in development).
    pub fn reload(&mut self) -> AppResult<()> {
        let glob = format!("{}/**/*", self.template_dir);
        let mut tera = Tera::new(&glob)
            .map_err(|e| AppError::Internal(format!("Template reload failed: {}", e)))?;
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
        self.tera
            .get_template_names()
            .map(|s| s.to_string())
            .collect()
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

impl Renderer for TemplateEngine {
    fn render(&self, name: &str, context: &Value) -> AppResult<String> {
        let ctx = Context::from_value(context.clone())
            .map_err(|e| AppError::Internal(format!("Invalid template context: {e}")))?;
        self.tera
            .render(name, &ctx)
            .map_err(|e| AppError::Internal(format!("Template render error '{name}': {e}")))
    }

    fn render_str(&self, source: &str, context: &Value) -> AppResult<String> {
        let ctx = Context::from_value(context.clone())
            .map_err(|e| AppError::Internal(format!("Invalid template context: {e}")))?;
        Tera::one_off(source, &ctx, false)
            .map_err(|e| AppError::Internal(format!("Template string render error: {e}")))
    }

    fn has_template(&self, name: &str) -> bool {
        self.tera.get_template_names().any(|n| n == name)
    }

    fn template_names(&self) -> Vec<String> {
        self.tera.get_template_names().map(String::from).collect()
    }
}
