//! HTML sanitization (via [ammonia](https://docs.rs/ammonia)) for untrusted
//! user content rendered as raw HTML (e.g. rich-text fields, `{@html}`).

/// HTML sanitization helpers.
pub struct Html;

impl Html {
    /// Sanitize untrusted HTML down to a safe subset: removes `<script>`,
    /// inline event handlers, `javascript:` URLs, etc., while keeping common
    /// formatting tags (`<b>`, `<a>`, `<p>`, …).
    pub fn sanitize(html: &str) -> String {
        ammonia::clean(html)
    }

    /// Strip **all** tags, keeping only the text content.
    pub fn strip_tags(html: &str) -> String {
        ammonia::Builder::new()
            .tags(std::collections::HashSet::new())
            .clean(html)
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_scripts_keeps_formatting() {
        let dirty =
            r#"<p>Hi <b>bold</b><script>alert(1)</script><a href="javascript:evil()">x</a></p>"#;
        let clean = Html::sanitize(dirty);
        assert!(clean.contains("<b>bold</b>"));
        assert!(!clean.contains("<script>"));
        assert!(!clean.contains("javascript:"));
    }

    #[test]
    fn strip_tags_keeps_text() {
        assert_eq!(Html::strip_tags("<p>Hello <b>world</b></p>"), "Hello world");
    }
}
