use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A value that never shows up in logs, panics or JSON: `Debug`, `Display` and
/// `Serialize` all print `[REDACTED]`. Read it with [`expose`](Self::expose), named so
/// that every place handling the secret in clear is easy to find.
///
/// ```ignore
/// #[derive(Deserialize)]
/// struct CreateFtpForm {
///     user: String,
///     password: Secret<String>,
/// }
///
/// api.create(&form.user, form.password.expose()).await?;
/// ```
#[derive(Clone)]
pub struct Secret<T>(T);

impl<T> Secret<T> {
    pub fn new(value: T) -> Self {
        Self(value)
    }

    /// The secret in clear.
    pub fn expose(&self) -> &T {
        &self.0
    }

    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> fmt::Debug for Secret<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Secret(<{}>)", std::any::type_name::<T>())
    }
}

impl<T> fmt::Display for Secret<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}

impl<T> Serialize for Secret<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str("[REDACTED]")
    }
}

/// Transparent: an incoming password must be readable.
impl<'de, T: Deserialize<'de>> Deserialize<'de> for Secret<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self(T::deserialize(deserializer)?))
    }
}

/// Constant-time comparison.
impl PartialEq for Secret<String> {
    fn eq(&self, other: &Self) -> bool {
        super::Crypto::tokens_match(&self.0, &other.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_printed_in_clear() {
        let secret: Secret<String> = Secret::new("my-secret-password".into());

        assert!(!format!("{secret:?}").contains("my-secret-password"));
        assert_eq!(format!("{secret}"), "[REDACTED]");
        assert_eq!(serde_json::to_string(&secret).unwrap(), "\"[REDACTED]\"");
    }

    #[test]
    fn readable_when_deserialized_and_exposed() {
        let secret: Secret<String> = serde_json::from_str("\"my-pwd\"").unwrap();

        assert_eq!(secret.expose(), "my-pwd");
        assert!(secret == Secret::new("my-pwd".to_string()));
        assert!(secret != Secret::new("other".to_string()));
    }
}
