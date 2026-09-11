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
    /// Numeric ID of the user who is really acting, when the token was minted for a
    /// support session in someone else's account (impersonation).
    ///
    /// Without it, a token minted for an administrator acting as a customer is
    /// indistinguishable from the customer's own: every action taken with it is logged
    /// under the customer's name. Absent from ordinary tokens, and from every token
    /// issued before this field existed — those still decode (`serde(default)`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub impersonator_id: Option<i64>,
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
        self.generate_with_impersonator(sub, username, roles, user_id, user_uuid, aud, None)
    }

    /// Generate a JWT token that records who is really acting.
    ///
    /// `user_id` / `user_uuid` identify the account the token opens; `impersonator_id`
    /// identifies the user acting inside it (an administrator in a support session).
    /// Pass `None` for an ordinary token — which is exactly what `generate_full` does.
    #[allow(clippy::too_many_arguments)]
    pub fn generate_with_impersonator(
        &self,
        sub: &str,
        username: &str,
        roles: Vec<String>,
        user_id: Option<i64>,
        user_uuid: Option<String>,
        aud: Option<String>,
        impersonator_id: Option<i64>,
    ) -> Result<String, jsonwebtoken::errors::Error> {
        let now = Utc::now().timestamp();
        let claims = Claims {
            sub: sub.to_string(),
            username: username.to_string(),
            roles,
            user_id,
            user_uuid,
            aud,
            impersonator_id,
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

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "a-test-secret-long-enough-for-hmac-1234";

    #[test]
    fn impersonator_round_trips() {
        let jwt = JwtManager::new(SECRET, 60);
        let token = jwt
            .generate_with_impersonator(
                "client@example.com",
                "client@example.com",
                vec!["ROLE_USER".into()],
                Some(42),
                None,
                Some("user".into()),
                Some(7),
            )
            .unwrap();

        let claims = jwt.verify_with_audience(&token, "user").unwrap();
        assert_eq!(claims.user_id, Some(42));
        assert_eq!(claims.impersonator_id, Some(7));
    }

    #[test]
    fn ordinary_tokens_carry_no_impersonator() {
        let jwt = JwtManager::new(SECRET, 60);
        let token = jwt
            .generate_full(
                "a@example.com",
                "a@example.com",
                vec![],
                Some(1),
                None,
                None,
            )
            .unwrap();
        assert_eq!(jwt.verify(&token).unwrap().impersonator_id, None);
    }

    #[test]
    fn tokens_issued_before_the_field_still_decode() {
        // The claims shape of every release up to 0.3.8: no `impersonator_id` key.
        #[derive(Serialize)]
        struct LegacyClaims<'a> {
            sub: &'a str,
            username: &'a str,
            roles: Vec<String>,
            user_id: Option<i64>,
            exp: i64,
            iat: i64,
        }
        let now = Utc::now().timestamp();
        let legacy = LegacyClaims {
            sub: "a@example.com",
            username: "a@example.com",
            roles: vec![],
            user_id: Some(3),
            exp: now + 60,
            iat: now,
        };
        let token = encode(
            &Header::default(),
            &legacy,
            &EncodingKey::from_secret(SECRET.as_bytes()),
        )
        .unwrap();

        let claims = JwtManager::new(SECRET, 60).verify(&token).unwrap();
        assert_eq!(claims.user_id, Some(3));
        assert_eq!(claims.impersonator_id, None);
    }
}
