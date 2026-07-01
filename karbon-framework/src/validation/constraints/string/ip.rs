use crate::validation::constraints::{Constraint, ConstraintResult, ConstraintViolation};

/// IP address version to validate against.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum IpVersion {
    V4,
    V6,
    All,
}

/// Validates that a value is a valid IP address.
pub struct Ip {
    pub message: String,
    pub version: IpVersion,
}

impl Default for Ip {
    fn default() -> Self {
        Self {
            message: "This value is not a valid IP address.".to_string(),
            version: IpVersion::All,
        }
    }
}

impl Ip {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn v4() -> Self {
        Self {
            version: IpVersion::V4,
            ..Self::default()
        }
    }

    pub fn v6() -> Self {
        Self {
            version: IpVersion::V6,
            ..Self::default()
        }
    }

    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = message.into();
        self
    }

    fn is_valid_ipv4(value: &str) -> bool {
        let parts: Vec<&str> = value.split('.').collect();
        if parts.len() != 4 {
            return false;
        }
        for part in parts {
            if part.is_empty() || part.len() > 3 {
                return false;
            }
            // No leading zeros (except "0" itself)
            if part.len() > 1 && part.starts_with('0') {
                return false;
            }
            match part.parse::<u16>() {
                Ok(n) if n <= 255 => {}
                _ => return false,
            }
        }
        true
    }

    fn is_valid_ipv6(value: &str) -> bool {
        if value.is_empty() {
            return false;
        }

        // Check for triple colon (invalid)
        if value.contains(":::") {
            return false;
        }

        // Handle :: abbreviation
        let double_colon_count = value.matches("::").count();
        if double_colon_count > 1 {
            return false;
        }

        let groups: Vec<&str> = value.split(':').collect();

        if double_colon_count == 1 {
            if groups.len() > 8 {
                return false;
            }
        } else if groups.len() != 8 {
            return false;
        }

        for group in &groups {
            if group.is_empty() {
                continue; // Part of ::
            }
            if group.len() > 4 {
                return false;
            }
            if !group.chars().all(|c| c.is_ascii_hexdigit()) {
                return false;
            }
        }

        true
    }

    fn is_valid(&self, value: &str) -> bool {
        match self.version {
            IpVersion::V4 => Self::is_valid_ipv4(value),
            IpVersion::V6 => Self::is_valid_ipv6(value),
            IpVersion::All => Self::is_valid_ipv4(value) || Self::is_valid_ipv6(value),
        }
    }
}

impl Constraint for Ip {
    fn validate(&self, value: &str) -> ConstraintResult {
        if !self.is_valid(value) {
            return Err(ConstraintViolation::new(self.name(), &self.message, value));
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        "Ip"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_ipv4() {
        let constraint = Ip::v4();
        assert!(constraint.validate("127.0.0.1").is_ok());
        assert!(constraint.validate("192.168.1.1").is_ok());
        assert!(constraint.validate("0.0.0.0").is_ok());
        assert!(constraint.validate("255.255.255.255").is_ok());
    }

    #[test]
    fn test_invalid_ipv4() {
        let constraint = Ip::v4();
        assert!(constraint.validate("256.0.0.1").is_err());
        assert!(constraint.validate("1.2.3").is_err());
        assert!(constraint.validate("1.2.3.4.5").is_err());
        assert!(constraint.validate("01.02.03.04").is_err()); // leading zeros
        assert!(constraint.validate("abc.def.ghi.jkl").is_err());
    }

    #[test]
    fn test_valid_ipv6() {
        let constraint = Ip::v6();
        assert!(constraint.validate("::1").is_ok());
        assert!(
            constraint
                .validate("2001:0db8:85a3:0000:0000:8a2e:0370:7334")
                .is_ok()
        );
        assert!(constraint.validate("fe80::1").is_ok());
        assert!(constraint.validate("::").is_ok());
    }

    #[test]
    fn test_invalid_ipv6() {
        let constraint = Ip::v6();
        assert!(constraint.validate("127.0.0.1").is_err());
        assert!(constraint.validate(":::1").is_err());
        assert!(constraint.validate("2001:db8::85a3::7334").is_err()); // double ::
    }

    #[test]
    fn test_all_versions() {
        let constraint = Ip::new();
        assert!(constraint.validate("127.0.0.1").is_ok());
        assert!(constraint.validate("::1").is_ok());
        assert!(constraint.validate("not-an-ip").is_err());
    }
}
