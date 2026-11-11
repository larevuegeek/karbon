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

/// Placeholder secrets shipped in templates/examples. They are published with Karbon's
/// source, so a token signed with one of them proves nothing: never valid, anywhere.
pub const PLACEHOLDER_SECRETS: &[&str] =
    &["change-me-to-a-secure-random-string", "change-me-to-a-random-secret", "change-me", "secret", "changeme"];

/// Returns true if the secret is one of the published [`PLACEHOLDER_SECRETS`]. Anyone can
/// sign a token with it, in any environment — unlike a merely short secret, which is still
/// private.
pub fn is_placeholder_secret(secret: &str) -> bool {
    PLACEHOLDER_SECRETS.contains(&secret.trim())
}

/// Returns true if the secret is unusable (empty) or obviously weak (too short or a
/// known placeholder). Used to fail closed instead of silently accepting forgeable tokens.
pub fn is_weak_secret(secret: &str) -> bool {
    secret.trim().len() < MIN_JWT_SECRET_LEN || is_placeholder_secret(secret)
}

/// JWT token manager
#[derive(Clone)]
pub struct JwtManager {
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
    expiration: i64,
    /// An empty secret or a published placeholder: no token is issued or accepted.
    unusable: bool,
}

impl JwtManager {
    /// Create a new JWT manager with the given secret and expiration (in seconds).
    ///
    /// With an empty secret or one of the [`PLACEHOLDER_SECRETS`], the manager is inert:
    /// it signs nothing and rejects every token. The check lives here and not only in
    /// `App::serve()` so that an application building its own router is covered too.
    pub fn new(secret: &str, expiration: i64) -> Self {
        Self {
            encoding_key: EncodingKey::from_secret(secret.as_bytes()),
            decoding_key: DecodingKey::from_secret(secret.as_bytes()),
            expiration,
            unusable: secret.is_empty() || is_placeholder_secret(secret),
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
        if self.unusable {
            return Err(jsonwebtoken::errors::ErrorKind::InvalidKeyFormat.into());
        }
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
    /// Fails closed when the secret is empty or a published placeholder: either key is
    /// known to everyone, so a token signed with it could be forged at will. Such a secret
    /// means "auth disabled" — every token is rejected, never accepted.
    pub fn verify(&self, token: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
        if self.unusable {
            return Err(jsonwebtoken::errors::ErrorKind::InvalidKeyFormat.into());
        }
        let validation = Validation { validate_aud: false, ..Default::default() };
        let token_data = decode::<Claims>(token, &self.decoding_key, &validation)?;
        Ok(token_data.claims)
    }

    /// Validate a token and require the given audience (`aud`) claim to match.
    /// Use this for audience-scoped tokens; `verify()` alone does not check `aud`.
    pub fn verify_with_audience(&self, token: &str, audience: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
        if self.unusable {
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
            .generate_full("a@example.com", "a@example.com", vec![], Some(1), None, None)
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
        let token = encode(&Header::default(), &legacy, &EncodingKey::from_secret(SECRET.as_bytes())).unwrap();

        let claims = JwtManager::new(SECRET, 60).verify(&token).unwrap();
        assert_eq!(claims.user_id, Some(3));
        assert_eq!(claims.impersonator_id, None);
    }

    /// The scaffolded `JWT_SECRET` is published with the CLI's source: a super-admin token
    /// signed with it used to pass `verify` in every environment but production.
    #[test]
    fn a_token_signed_with_a_published_placeholder_is_never_accepted() {
        for placeholder in PLACEHOLDER_SECRETS {
            let now = Utc::now().timestamp();
            let forged = encode(
                &Header::default(),
                &Claims {
                    sub: "attacker".into(),
                    username: "attacker".into(),
                    roles: vec!["ROLE_SUPER_ADMIN".into()],
                    user_id: Some(1),
                    user_uuid: None,
                    aud: Some("user".into()),
                    impersonator_id: None,
                    exp: now + 3600,
                    iat: now,
                },
                &EncodingKey::from_secret(placeholder.as_bytes()),
            )
            .unwrap();

            let jwt = JwtManager::new(placeholder, 60);
            assert!(jwt.verify(&forged).is_err(), "{placeholder}");
            assert!(jwt.verify_with_audience(&forged, "user").is_err(), "{placeholder}");
            assert!(jwt.generate("a", "a", vec![]).is_err(), "{placeholder}");
        }
    }

    #[test]
    fn an_empty_secret_issues_and_accepts_nothing() {
        let jwt = JwtManager::new("", 60);
        assert!(jwt.generate("a", "a", vec![]).is_err());
        let token = JwtManager::new(SECRET, 60).generate("a", "a", vec![]).unwrap();
        assert!(jwt.verify(&token).is_err());
    }

    #[test]
    fn a_placeholder_is_weak_but_a_short_secret_is_not_a_placeholder() {
        assert!(is_placeholder_secret("change-me-to-a-random-secret"));
        assert!(is_placeholder_secret("  secret  "));
        assert!(!is_placeholder_secret("short-but-private"));
        assert!(is_weak_secret("short-but-private"));
        assert!(!is_weak_secret(SECRET));
    }
}
