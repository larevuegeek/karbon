use crate::validation::constraints::{Constraint, ConstraintResult, ConstraintViolation};

/// Validates that a value is a valid URL.
///
/// Equivalent to Symfony's `Url` constraint.
pub struct Url {
    pub message: String,
    pub protocols: Vec<String>,
}

impl Default for Url {
    fn default() -> Self {
        Self {
            message: "This value is not a valid URL.".to_string(),
            protocols: vec!["http".to_string(), "https".to_string()],
        }
    }
}

impl Url {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = message.into();
        self
    }

    pub fn with_protocols(mut self, protocols: Vec<String>) -> Self {
        self.protocols = protocols;
        self
    }

    fn is_valid_url(&self, value: &str) -> bool {
        // Check protocol
        let protocol_end = match value.find("://") {
            Some(pos) => pos,
            None => return false,
        };

        let protocol = &value[..protocol_end];
        if !self.protocols.iter().any(|p| p == protocol) {
            return false;
        }

        let rest = &value[protocol_end + 3..];
        if rest.is_empty() {
            return false;
        }

        // Extract host (before path, query, fragment)
        let host_end = rest.find('/').or_else(|| rest.find('?')).or_else(|| rest.find('#')).unwrap_or(rest.len());
        let host_part = &rest[..host_end];

        if host_part.is_empty() {
            return false;
        }

        // Handle port
        let host = if let Some(colon_pos) = host_part.rfind(':') {
            let port_str = &host_part[colon_pos + 1..];
            if !port_str.is_empty() && port_str.parse::<u16>().is_err() {
                return false;
            }
            &host_part[..colon_pos]
        } else {
            host_part
        };

        if host.is_empty() {
            return false;
        }

        // Validate host
        for part in host.split('.') {
            if part.is_empty() || part.len() > 63 {
                return false;
            }
        }

        true
    }
}

impl Constraint for Url {
    fn validate(&self, value: &str) -> ConstraintResult {
        if !self.is_valid_url(value) {
            return Err(ConstraintViolation::new(
                self.name(),
                &self.message,
                value,
            ));
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        "Url"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_urls() {
        let constraint = Url::new();
        assert!(constraint.validate("http://example.com").is_ok());
        assert!(constraint.validate("https://example.com").is_ok());
        assert!(constraint.validate("https://example.com/path").is_ok());
        assert!(constraint.validate("https://example.com:8080").is_ok());
        assert!(constraint.validate("https://sub.domain.example.com").is_ok());
        assert!(constraint.validate("https://example.com/path?query=1").is_ok());
        assert!(constraint.validate("https://example.com/path#fragment").is_ok());
    }

    #[test]
    fn test_invalid_urls() {
        let constraint = Url::new();
        assert!(constraint.validate("").is_err());
        assert!(constraint.validate("not-a-url").is_err());
        assert!(constraint.validate("ftp://example.com").is_err()); // ftp not in default protocols
        assert!(constraint.validate("://example.com").is_err());
        assert!(constraint.validate("http://").is_err());
    }

    #[test]
    fn test_custom_protocols() {
        let constraint = Url::new().with_protocols(vec!["ftp".to_string(), "ssh".to_string()]);
        assert!(constraint.validate("ftp://example.com").is_ok());
        assert!(constraint.validate("ssh://example.com").is_ok());
        assert!(constraint.validate("http://example.com").is_err());
    }
}
