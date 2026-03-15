use crate::validation::constraints::{Constraint, ConstraintResult, ConstraintViolation};

/// Validates that a value is a valid hostname (RFC 1123).
pub struct Hostname {
    pub message: String,
    pub require_tld: bool,
}

impl Default for Hostname {
    fn default() -> Self {
        Self {
            message: "This value is not a valid hostname.".to_string(),
            require_tld: true,
        }
    }
}

impl Hostname {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = message.into();
        self
    }

    pub fn require_tld(mut self, require: bool) -> Self {
        self.require_tld = require;
        self
    }

    fn is_valid_hostname(&self, value: &str) -> bool {
        if value.is_empty() || value.len() > 253 {
            return false;
        }

        let labels: Vec<&str> = value.split('.').collect();

        if self.require_tld && labels.len() < 2 {
            return false;
        }

        for label in &labels {
            if label.is_empty() || label.len() > 63 {
                return false;
            }
            if label.starts_with('-') || label.ends_with('-') {
                return false;
            }
            if !label.chars().all(|c| c.is_alphanumeric() || c == '-') {
                return false;
            }
        }

        // TLD must not be all-numeric
        if self.require_tld {
            if let Some(tld) = labels.last() {
                if tld.chars().all(|c| c.is_ascii_digit()) {
                    return false;
                }
            }
        }

        true
    }
}

impl Constraint for Hostname {
    fn validate(&self, value: &str) -> ConstraintResult {
        if !self.is_valid_hostname(value) {
            return Err(ConstraintViolation::new(
                self.name(),
                &self.message,
                value,
            ));
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        "Hostname"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_hostnames() {
        let constraint = Hostname::new();
        assert!(constraint.validate("example.com").is_ok());
        assert!(constraint.validate("sub.example.com").is_ok());
        assert!(constraint.validate("my-host.example.org").is_ok());
        assert!(constraint.validate("a.io").is_ok());
    }

    #[test]
    fn test_invalid_hostnames() {
        let constraint = Hostname::new();
        assert!(constraint.validate("").is_err());
        assert!(constraint.validate("-example.com").is_err());
        assert!(constraint.validate("example-.com").is_err());
        assert!(constraint.validate("exam ple.com").is_err());
        assert!(constraint.validate("example.123").is_err()); // numeric TLD
    }

    #[test]
    fn test_without_tld_requirement() {
        let constraint = Hostname::new().require_tld(false);
        assert!(constraint.validate("localhost").is_ok());
        assert!(constraint.validate("myserver").is_ok());
    }

    #[test]
    fn test_with_tld_requirement() {
        let constraint = Hostname::new();
        assert!(constraint.validate("localhost").is_err());
    }
}
