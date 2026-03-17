//! Mail template helper types.

/// Options for sending a templated email.
#[derive(Debug, Clone)]
pub struct MailTemplateOptions {
    pub to: String,
    pub subject: String,
    pub reply_to: Option<String>,
}

impl Default for MailTemplateOptions {
    fn default() -> Self {
        Self {
            to: String::new(),
            subject: String::new(),
            reply_to: None,
        }
    }
}

impl MailTemplateOptions {
    pub fn new(to: &str, subject: &str) -> Self {
        Self {
            to: to.to_string(),
            subject: subject.to_string(),
            reply_to: None,
        }
    }
}
