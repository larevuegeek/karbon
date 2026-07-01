use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    message::{Attachment, Mailbox, MultiPart, SinglePart, header::ContentType},
    transport::smtp::authentication::Credentials,
};

use crate::config::Config;
use crate::error::{AppError, AppResult};

/// Email sender using SMTP
#[derive(Clone)]
pub struct Mailer {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    from: String,
}

impl Mailer {
    /// Create a new mailer from app config
    pub fn new(config: &Config) -> AppResult<Self> {
        let credentials = Credentials::new(config.smtp_user.clone(), config.smtp_password.clone());

        let transport = AsyncSmtpTransport::<Tokio1Executor>::relay(&config.smtp_host)
            .map_err(|e| AppError::Internal(format!("SMTP config error: {}", e)))?
            .credentials(credentials)
            .port(config.smtp_port)
            .build();

        Ok(Self {
            transport,
            from: config.mail_from.clone(),
        })
    }

    /// Start building an email with this mailer's transport and from address
    pub fn compose(&self) -> MailBuilder {
        MailBuilder {
            transport: self.transport.clone(),
            from: self.from.clone(),
            reply_to: None,
            to: Vec::new(),
            cc: Vec::new(),
            bcc: Vec::new(),
            subject: String::new(),
            text_body: None,
            html_body: None,
            attachments: Vec::new(),
            priority: Priority::Normal,
        }
    }

    /// Send a plain text email (shortcut, backward compatible)
    pub async fn send_text(&self, to: &str, subject: &str, body: &str) -> AppResult<()> {
        self.compose()
            .to(to)?
            .subject(subject)
            .text(body)
            .send()
            .await
    }

    /// Send an HTML email (shortcut, backward compatible)
    pub async fn send_html(&self, to: &str, subject: &str, html: &str) -> AppResult<()> {
        self.compose()
            .to(to)?
            .subject(subject)
            .html(html)
            .send()
            .await
    }
}

/// Email priority
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    High,
    Normal,
    Low,
}

/// An email attachment
#[derive(Debug, Clone)]
pub struct MailAttachment {
    pub filename: String,
    pub content_type: String,
    pub data: Vec<u8>,
}

impl MailAttachment {
    /// Create an attachment from raw bytes
    pub fn new(
        filename: impl Into<String>,
        content_type: impl Into<String>,
        data: Vec<u8>,
    ) -> Self {
        Self {
            filename: filename.into(),
            content_type: content_type.into(),
            data,
        }
    }

    /// Create an attachment by reading a file from disk
    pub fn from_file(path: &std::path::Path) -> AppResult<Self> {
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("attachment")
            .to_string();

        let data = std::fs::read(path).map_err(|e| {
            AppError::Internal(format!("Failed to read attachment '{}': {}", filename, e))
        })?;

        let content_type = Self::guess_content_type(&filename);

        Ok(Self {
            filename,
            content_type,
            data,
        })
    }

    fn guess_content_type(filename: &str) -> String {
        let ext = filename.rsplit('.').next().unwrap_or("").to_lowercase();

        match ext.as_str() {
            "pdf" => "application/pdf",
            "zip" => "application/zip",
            "gz" | "gzip" => "application/gzip",
            "doc" => "application/msword",
            "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            "xls" => "application/vnd.ms-excel",
            "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            "csv" => "text/csv",
            "txt" => "text/plain",
            "html" | "htm" => "text/html",
            "json" => "application/json",
            "xml" => "application/xml",
            "jpg" | "jpeg" => "image/jpeg",
            "png" => "image/png",
            "gif" => "image/gif",
            "webp" => "image/webp",
            "svg" => "image/svg+xml",
            "mp4" => "video/mp4",
            "mp3" => "audio/mpeg",
            _ => "application/octet-stream",
        }
        .to_string()
    }
}

/// Email builder with fluent API
pub struct MailBuilder {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    from: String,
    reply_to: Option<String>,
    to: Vec<String>,
    cc: Vec<String>,
    bcc: Vec<String>,
    subject: String,
    text_body: Option<String>,
    html_body: Option<String>,
    attachments: Vec<MailAttachment>,
    priority: Priority,
}

fn parse_mailbox(addr: &str) -> AppResult<Mailbox> {
    addr.parse::<Mailbox>()
        .map_err(|e| AppError::BadRequest(format!("Invalid email address '{}': {}", addr, e)))
}

impl MailBuilder {
    /// Override the sender (FROM) address
    pub fn from(mut self, address: &str) -> Self {
        self.from = address.to_string();
        self
    }

    /// Add a recipient (TO)
    pub fn to(mut self, address: &str) -> AppResult<Self> {
        parse_mailbox(address)?;
        self.to.push(address.to_string());
        Ok(self)
    }

    /// Add multiple recipients (TO)
    pub fn to_many(mut self, addresses: &[&str]) -> AppResult<Self> {
        for addr in addresses {
            parse_mailbox(addr)?;
            self.to.push(addr.to_string());
        }
        Ok(self)
    }

    /// Add a CC recipient
    pub fn cc(mut self, address: &str) -> AppResult<Self> {
        parse_mailbox(address)?;
        self.cc.push(address.to_string());
        Ok(self)
    }

    /// Add multiple CC recipients
    pub fn cc_many(mut self, addresses: &[&str]) -> AppResult<Self> {
        for addr in addresses {
            parse_mailbox(addr)?;
            self.cc.push(addr.to_string());
        }
        Ok(self)
    }

    /// Add a BCC recipient
    pub fn bcc(mut self, address: &str) -> AppResult<Self> {
        parse_mailbox(address)?;
        self.bcc.push(address.to_string());
        Ok(self)
    }

    /// Add multiple BCC recipients
    pub fn bcc_many(mut self, addresses: &[&str]) -> AppResult<Self> {
        for addr in addresses {
            parse_mailbox(addr)?;
            self.bcc.push(addr.to_string());
        }
        Ok(self)
    }

    /// Set a Reply-To address
    pub fn reply_to(mut self, address: &str) -> AppResult<Self> {
        parse_mailbox(address)?;
        self.reply_to = Some(address.to_string());
        Ok(self)
    }

    /// Set the subject
    pub fn subject(mut self, subject: &str) -> Self {
        self.subject = subject.to_string();
        self
    }

    /// Set the plain text body
    pub fn text(mut self, body: &str) -> Self {
        self.text_body = Some(body.to_string());
        self
    }

    /// Set the HTML body
    pub fn html(mut self, body: &str) -> Self {
        self.html_body = Some(body.to_string());
        self
    }

    /// Set both plain text and HTML body (multipart/alternative)
    pub fn text_and_html(mut self, text: &str, html: &str) -> Self {
        self.text_body = Some(text.to_string());
        self.html_body = Some(html.to_string());
        self
    }

    /// Add an attachment from bytes
    pub fn attach(mut self, attachment: MailAttachment) -> Self {
        self.attachments.push(attachment);
        self
    }

    /// Add an attachment from a file path
    pub fn attach_file(mut self, path: &std::path::Path) -> AppResult<Self> {
        self.attachments.push(MailAttachment::from_file(path)?);
        Ok(self)
    }

    /// Set email priority
    pub fn priority(mut self, priority: Priority) -> Self {
        self.priority = priority;
        self
    }

    /// Build and send the email
    pub async fn send(self) -> AppResult<()> {
        if self.to.is_empty() {
            return Err(AppError::BadRequest("No recipients specified".to_string()));
        }

        if self.subject.is_empty() {
            return Err(AppError::BadRequest("No subject specified".to_string()));
        }

        if self.text_body.is_none() && self.html_body.is_none() {
            return Err(AppError::BadRequest("No email body specified".to_string()));
        }

        let transport = self.transport.clone();
        let message = self.build_message()?;

        transport
            .send(message)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to send email: {}", e)))?;

        Ok(())
    }

    fn build_message(self) -> AppResult<Message> {
        let mut builder = Message::builder().from(parse_mailbox(&self.from)?);

        // Recipients
        for addr in &self.to {
            builder = builder.to(parse_mailbox(addr)?);
        }
        for addr in &self.cc {
            builder = builder.cc(parse_mailbox(addr)?);
        }
        for addr in &self.bcc {
            builder = builder.bcc(parse_mailbox(addr)?);
        }

        // Reply-To
        if let Some(reply_to) = &self.reply_to {
            builder = builder.reply_to(parse_mailbox(reply_to)?);
        }

        // Subject
        builder = builder.subject(&self.subject);

        // Build body
        let has_attachments = !self.attachments.is_empty();
        let has_text = self.text_body.is_some();
        let has_html = self.html_body.is_some();

        if has_attachments {
            // Mixed multipart: body + attachments
            let body_part =
                Self::make_body_part(&self.text_body, &self.html_body, has_text, has_html);

            let mut mixed = MultiPart::mixed().multipart(body_part);

            for att in self.attachments {
                let content_type = ContentType::parse(&att.content_type)
                    .unwrap_or(ContentType::parse("application/octet-stream").unwrap());
                let attachment_part = Attachment::new(att.filename).body(att.data, content_type);
                mixed = mixed.singlepart(attachment_part);
            }

            builder
                .multipart(mixed)
                .map_err(|e| AppError::Internal(format!("Email build error: {}", e)))
        } else if has_text && has_html {
            let alternative =
                MultiPart::alternative_plain_html(self.text_body.unwrap(), self.html_body.unwrap());
            builder
                .multipart(alternative)
                .map_err(|e| AppError::Internal(format!("Email build error: {}", e)))
        } else if has_html {
            builder
                .header(ContentType::TEXT_HTML)
                .body(self.html_body.unwrap())
                .map_err(|e| AppError::Internal(format!("Email build error: {}", e)))
        } else {
            builder
                .header(ContentType::TEXT_PLAIN)
                .body(self.text_body.unwrap())
                .map_err(|e| AppError::Internal(format!("Email build error: {}", e)))
        }
    }

    fn make_body_part(
        text_body: &Option<String>,
        html_body: &Option<String>,
        has_text: bool,
        has_html: bool,
    ) -> MultiPart {
        if has_text && has_html {
            MultiPart::alternative_plain_html(
                text_body.clone().unwrap(),
                html_body.clone().unwrap(),
            )
        } else if has_html {
            MultiPart::alternative().singlepart(SinglePart::html(html_body.clone().unwrap()))
        } else {
            MultiPart::alternative().singlepart(SinglePart::plain(text_body.clone().unwrap()))
        }
    }
}
