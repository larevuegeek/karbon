use argon2::{
    Argon2,
    password_hash::{PasswordHasher, PasswordVerifier, phc::PasswordHash},
};

/// Password hashing and verification using Argon2id
/// Also supports legacy bcrypt hashes ($2y$/$2b$) from Symfony
pub struct Password;

impl Password {
    /// Hash a password with Argon2id
    pub fn hash(password: &str) -> Result<String, argon2::password_hash::Error> {
        // argon2 0.6 generates the salt itself (`getrandom`, on by default); passing one
        // explicitly is `hash_password_with_salt`.
        let hash = Argon2::default().hash_password(password.as_bytes())?;
        Ok(hash.to_string())
    }

    /// Verify a password against a hash — supports bcrypt ($2a$/$2y$/$2b$) and Argon2id.
    pub fn verify(password: &str, hash: &str) -> Result<bool, argon2::password_hash::Error> {
        if Self::is_legacy_bcrypt(hash) {
            // bcrypt::verify → Ok(false) for a wrong password, Err only for a malformed
            // stored hash. Surface the latter as an error instead of "wrong password".
            bcrypt::verify(password, hash).map_err(|_| argon2::password_hash::Error::Algorithm)
        } else {
            let parsed_hash = PasswordHash::new(hash)?;
            Ok(Argon2::default()
                .verify_password(password.as_bytes(), &parsed_hash)
                .is_ok())
        }
    }

    /// True if `hash` is a legacy bcrypt hash (`$2a$`, `$2y$`, `$2b$`).
    pub fn is_legacy_bcrypt(hash: &str) -> bool {
        hash.starts_with("$2a$") || hash.starts_with("$2y$") || hash.starts_with("$2b$")
    }

    /// Whether a stored hash should be re-hashed to Argon2id on the next successful login
    /// (i.e. it's a legacy bcrypt hash). Call after `verify()` returns `Ok(true)` to
    /// transparently upgrade migrated accounts.
    pub fn needs_rehash(hash: &str) -> bool {
        Self::is_legacy_bcrypt(hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Produced by argon2 0.5 — the version Karbon hashed with up to 0.3.9. Stored
    /// password hashes must keep verifying across the 0.6 upgrade, or every account
    /// locks out.
    const ARGON2_0_5_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$a2FyYm9uLXRlc3Qtc2FsdA$zwsjs2lz+YeLe27RdXKnLEOs8eYCUf9Z+eWetM8K9jg";
    const ARGON2_0_5_PASSWORD: &str = "correct horse battery staple";

    #[test]
    fn hashes_from_argon2_0_5_still_verify() {
        assert!(Password::verify(ARGON2_0_5_PASSWORD, ARGON2_0_5_HASH).unwrap());
        assert!(!Password::verify("wrong password", ARGON2_0_5_HASH).unwrap());
    }

    #[test]
    fn round_trips_and_salts_each_hash() {
        let a = Password::hash("s3cret").unwrap();
        let b = Password::hash("s3cret").unwrap();
        assert_ne!(a, b, "each hash must carry its own random salt");
        assert!(a.starts_with("$argon2id$"));
        assert!(Password::verify("s3cret", &a).unwrap());
        assert!(!Password::verify("s3cret ", &a).unwrap());
    }

    #[test]
    fn legacy_bcrypt_is_detected_and_verified() {
        let hash = bcrypt::hash("s3cret", 4).unwrap().replace("$2b$", "$2y$");
        assert!(Password::is_legacy_bcrypt(&hash));
        assert!(Password::needs_rehash(&hash));
        assert!(Password::verify("s3cret", &hash).unwrap());
        assert!(!Password::verify("nope", &hash).unwrap());
    }
}
