use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
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

    /// Verify a password against a hash — supports bcrypt ($2y$/$2b$) and Argon2id
    pub fn verify(password: &str, hash: &str) -> Result<bool, argon2::password_hash::Error> {
        if hash.starts_with("$2y$") || hash.starts_with("$2b$") {
            Ok(bcrypt::verify(password, hash).unwrap_or(false))
        } else {
            let parsed_hash = PasswordHash::new(hash)?;
            Ok(Argon2::default()
                .verify_password(password.as_bytes(), &parsed_hash)
                .is_ok())
        }
    }
}
