use chrono::Utc;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
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

/// JWT token manager
#[derive(Clone)]
pub struct JwtManager {
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
    expiration: i64,
}

impl JwtManager {
    /// Create a new JWT manager with the given secret and expiration (in seconds)
    pub fn new(secret: &str, expiration: i64) -> Self {
        Self {
            encoding_key: EncodingKey::from_secret(secret.as_bytes()),
            decoding_key: DecodingKey::from_secret(secret.as_bytes()),
            expiration,
        }
    }

    /// Generate a JWT token for a user (basic — sub only)
    pub fn generate(&self, sub: &str, username: &str, roles: Vec<String>) -> Result<String, jsonwebtoken::errors::Error> {
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

    /// Validate and decode a JWT token
    pub fn verify(&self, token: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
        let mut validation = Validation::default();
        validation.validate_aud = false;
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
