use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::RngExt;
use sha2::{Sha256, Digest};

/// Cryptographic helpers
pub struct Crypto;

impl Crypto {
    /// Generate a random token (URL-safe base64)
    pub fn random_token(length: usize) -> String {
        let mut bytes = vec![0u8; length];
        rand::rng().fill(&mut bytes[..]);
        URL_SAFE_NO_PAD.encode(&bytes)
    }

    /// Hash a token with SHA-256 (for storing refresh tokens)
    pub fn hash_token(token: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(token.as_bytes());
        let result = hasher.finalize();
        result.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Generate a short alphanumeric code (e.g. for email verification)
    pub fn random_code(length: usize) -> String {
        const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
        let mut rng = rand::rng();
        (0..length)
            .map(|_| {
                let idx = rng.random_range(0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect()
    }
}
