use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};

/// Password hashing and verification using Argon2id
/// Also supports legacy bcrypt hashes ($2y$/$2b$) from Symfony
pub struct Password;

impl Password {
    /// Hash a password with Argon2id
    pub fn hash(password: &str) -> Result<String, argon2::password_hash::Error> {
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();
        let hash = argon2.hash_password(password.as_bytes(), &salt)?;
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
