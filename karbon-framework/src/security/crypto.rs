use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::RngExt;
use sha2::{Digest, Sha256};

/// Cryptographic helpers
pub struct Crypto;

impl Crypto {
    /// Generate a random token (URL-safe base64)
    pub fn random_token(length: usize) -> String {
        let mut bytes = vec![0u8; length];
        rand::rng().fill(&mut bytes[..]);
        URL_SAFE_NO_PAD.encode(&bytes)
    }

    /// Hash a token with SHA-256 (for storing refresh tokens).
    ///
    /// Note: prefer [`hash_token_keyed`](Self::hash_token_keyed) with a server-side secret
    /// so that a database dump alone doesn't yield usable token-hash lookups.
    pub fn hash_token(token: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(token.as_bytes());
        let result = hasher.finalize();
        result.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Keyed token hash (HMAC-SHA256) for storing refresh tokens. With a server-side `key`
    /// (e.g. `JWT_SECRET`), a leaked DB row can't be used to look tokens up without the key.
    pub fn hash_token_keyed(token: &str, key: &[u8]) -> String {
        hmac_sha256(key, token.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    /// Constant-time comparison of two hex/ASCII token hashes (avoids timing leaks when
    /// comparing a presented token hash against a stored one).
    pub fn tokens_match(a: &str, b: &str) -> bool {
        let (a, b) = (a.as_bytes(), b.as_bytes());
        if a.len() != b.len() {
            return false;
        }
        a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
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

/// HMAC-SHA256 (RFC 2104) implemented over sha2, to avoid pulling in an extra crate.
fn hmac_sha256(key: &[u8], msg: &[u8]) -> [u8; 32] {
    const BLOCK: usize = 64;
    let mut k = [0u8; BLOCK];
    if key.len() > BLOCK {
        let digest = Sha256::digest(key);
        k[..32].copy_from_slice(&digest);
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let mut ipad = [0x36u8; BLOCK];
    let mut opad = [0x5cu8; BLOCK];
    for i in 0..BLOCK {
        ipad[i] ^= k[i];
        opad[i] ^= k[i];
    }
    let inner = {
        let mut h = Sha256::new();
        h.update(ipad);
        h.update(msg);
        h.finalize()
    };
    let mut outer = Sha256::new();
    outer.update(opad);
    outer.update(inner);
    outer.finalize().into()
}
