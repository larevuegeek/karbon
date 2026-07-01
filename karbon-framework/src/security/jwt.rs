use chrono::Utc;
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};

/// JWT claims stored in the token
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Subject (username or email)
    pub sub: String,
    /// Username
    pub username: String,
    /// User roles
    pub roles: Vec<String>,
    /// Numeric user ID (optional, for apps using i64 primary keys)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_id: Option<i64>,
    /// User UUID (optional, for apps using a separate UUID column)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_uuid: Option<String>,
    /// Audience (optional, for multi-audience token validation)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aud: Option<String>,
    /// Expiration timestamp
    pub exp: i64,
    /// Issued at
    pub iat: i64,
}

/// Minimum accepted length (bytes) for a production JWT secret.
pub const MIN_JWT_SECRET_LEN: usize = 32;

/// Placeholder secrets shipped in templates/examples — never valid in production.
pub const PLACEHOLDER_SECRETS: &[&str] = &[
    "change-me-to-a-secure-random-string",
    "change-me-to-a-random-secret",
    "change-me",
    "secret",
    "changeme",
];

/// Returns true if the secret is unusable (empty) or obviously weak (too short or a
/// known placeholder). Used to fail closed instead of silently accepting forgeable tokens.
pub fn is_weak_secret(secret: &str) -> bool {
    secret.trim().len() < MIN_JWT_SECRET_LEN || PLACEHOLDER_SECRETS.contains(&secret.trim())
}

/// JWT token manager
#[derive(Clone)]
pub struct JwtManager {
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
    expiration: i64,
    /// When the secret is empty, verification always fails closed (auth disabled).
    secret_empty: bool,
}

impl JwtManager {
    /// Create a new JWT manager with the given secret and expiration (in seconds)
    pub fn new(secret: &str, expiration: i64) -> Self {
        Self {
            encoding_key: EncodingKey::from_secret(secret.as_bytes()),
            decoding_key: DecodingKey::from_secret(secret.as_bytes()),
            expiration,
            secret_empty: secret.is_empty(),
        }
    }

    /// Generate a JWT token for a user (basic — sub only)
    pub fn generate(
        &self,
        sub: &str,
        username: &str,
        roles: Vec<String>,
    ) -> Result<String, jsonwebtoken::errors::Error> {
        self.generate_full(sub, username, roles, None, None, None)
    }

    /// Generate a JWT token with numeric user_id and/or UUID
    pub fn generate_full(
        &self,
        sub: &str,
        username: &str,
        roles: Vec<String>,
        user_id: Option<i64>,
        user_uuid: Option<String>,
        aud: Option<String>,
    ) -> Result<String, jsonwebtoken::errors::Error> {
        let now = Utc::now().timestamp();
        let claims = Claims {
            sub: sub.to_string(),
            username: username.to_string(),
            roles,
            user_id,
            user_uuid,
            aud,
            exp: now + self.expiration,
            iat: now,
        };

        encode(&Header::default(), &claims, &self.encoding_key)
    }

    /// Validate and decode a JWT token.
    ///
    /// Fails closed when the secret is empty: an empty HMAC key would otherwise verify
    /// tokens signed with a publicly-known (empty) key, allowing trivial forgery. So an
    /// empty secret means "auth disabled" — every token is rejected, never accepted.
    pub fn verify(&self, token: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
        if self.secret_empty {
            return Err(jsonwebtoken::errors::ErrorKind::InvalidKeyFormat.into());
        }
        let mut validation = Validation::default();
        validation.validate_aud = false;
        let token_data = decode::<Claims>(token, &self.decoding_key, &validation)?;
        Ok(token_data.claims)
    }

    /// Validate a token and require the given audience (`aud`) claim to match.
    /// Use this for audience-scoped tokens; `verify()` alone does not check `aud`.
    pub fn verify_with_audience(
        &self,
        token: &str,
        audience: &str,
    ) -> Result<Claims, jsonwebtoken::errors::Error> {
        if self.secret_empty {
            return Err(jsonwebtoken::errors::ErrorKind::InvalidKeyFormat.into());
        }
        let mut validation = Validation::default();
        validation.set_audience(&[audience]);
        let token_data = decode::<Claims>(token, &self.decoding_key, &validation)?;
        Ok(token_data.claims)
    }

    /// Generate an opaque refresh token (random 48-byte string, NOT a JWT).
    /// The caller must hash it with `Crypto::hash_token()` before storing in DB.
    /// Returns the raw token to send to the client.
    pub fn generate_refresh_token() -> String {
        super::Crypto::random_token(48)
    }
}
