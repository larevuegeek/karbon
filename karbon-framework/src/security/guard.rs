use std::future::Future;
use std::pin::Pin;

use axum::{
    extract::{FromRequestParts, Request},
    http::request::Parts,
    middleware::Next,
    response::Response,
};

use crate::error::AppError;
use crate::http::AppState;

use super::{Claims, RoleHierarchy};

/// Extractor: pulls the authenticated user from the request.
///
/// The access token is read from `Authorization: Bearer <token>` (API clients) or, when
/// the request has no such header, from the session cookie (browsers — see
/// [`auth_cookie`](super::auth_cookie)).
///
/// Usage in a handler:
/// ```ignore
/// async fn my_handler(user: AuthGuard) -> impl IntoResponse {
///     format!("Hello {}", user.claims.username)
/// }
/// ```
#[derive(Debug, Clone)]
pub struct AuthGuard {
    pub claims: Claims,
    role_hierarchy: RoleHierarchy,
}

impl AuthGuard {
    /// Build an AuthGuard from existing Claims using the app's hierarchy
    /// ([`active_hierarchy`](crate::security::active_hierarchy): the one [`App`](crate::http::App)
    /// resolved at startup, or [`default_hierarchy`](crate::security::default_hierarchy) outside
    /// an app — the previous behaviour).
    ///
    /// When you have the `AppState`, [`from_claims_with`](Self::from_claims_with) with
    /// `state.role_hierarchy` says explicitly which hierarchy is used.
    pub fn from_claims(claims: Claims) -> Self {
        Self { claims, role_hierarchy: crate::security::active_hierarchy() }
    }

    /// Build an AuthGuard from existing Claims with an explicit role hierarchy (matches the
    /// app's configured `state.role_hierarchy`). Prefer this over [`from_claims`](Self::from_claims).
    pub fn from_claims_with(claims: Claims, role_hierarchy: RoleHierarchy) -> Self {
        Self { claims, role_hierarchy }
    }

    /// Check if the user has a specific role (with hierarchy resolution).
    /// E.g. a ROLE_SUPER_ADMIN user will pass `has_role("ROLE_ADMIN")`.
    pub fn has_role(&self, role: &str) -> bool {
        self.role_hierarchy.has_role(&self.claims.roles, role)
    }

    /// Require a specific role, return Forbidden if not
    pub fn require_role(&self, role: &str) -> Result<(), AppError> {
        if self.has_role(role) {
            Ok(())
        } else {
            Err(AppError::Forbidden(format!("Role '{}' required", role)))
        }
    }

    /// Get the subject (sub claim — usually email or string user ID)
    pub fn sub(&self) -> &str {
        &self.claims.sub
    }

    /// Get the numeric user ID (if set in token)
    pub fn user_id(&self) -> Option<i64> {
        self.claims.user_id
    }

    /// Get the user UUID (if set in token)
    pub fn user_uuid(&self) -> Option<&str> {
        self.claims.user_uuid.as_deref()
    }

    /// ID of the user really acting, when the token was minted for a support session
    /// in this user's account (see `Claims::impersonator_id`).
    pub fn impersonator_id(&self) -> Option<i64> {
        self.claims.impersonator_id
    }

    /// Get the username
    pub fn username(&self) -> &str {
        &self.claims.username
    }
}

impl FromRequestParts<AppState> for AuthGuard {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        use super::auth_cookie::{NoToken, token_from};

        let token: &str = token_from(&parts.headers).map_err(|reason| {
            AppError::Unauthorized(
                match reason {
                    NoToken::Missing => "Authentication required",
                    NoToken::Malformed => "Invalid Authorization format",
                }
                .to_string(),
            )
        })?;

        // Verify JWT
        let jwt: super::JwtManager = super::JwtManager::new(&state.config.jwt_secret, state.config.jwt_expiration);
        let claims: Claims = jwt
            .verify(token)
            .map_err(|_| AppError::Unauthorized("Invalid or expired token".to_string()))?;

        Ok(AuthGuard { claims, role_hierarchy: state.role_hierarchy.clone() })
    }
}

/// Middleware factory that protects a group of routes with a role.
///
/// ```ignore
/// router.layer(middleware::from_fn(require_role("ROLE_ADMIN")))
/// router.layer(middleware::from_fn(require_role("ROLE_REDACTEUR")))
/// ```
#[allow(clippy::type_complexity)]
pub fn require_role(
    role: &'static str,
) -> impl Fn(AuthGuard, Request, Next) -> Pin<Box<dyn Future<Output = Result<Response, AppError>> + Send>> + Clone {
    move |auth: AuthGuard, request: Request, next: Next| {
        Box::pin(async move {
            auth.require_role(role)?;
            Ok(next.run(request).await)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::db::Database;
    use crate::security::{AUTH_COOKIE, JwtManager, default_hierarchy};
    use axum::http::Request;

    const SECRET: &str = "a-test-secret-long-enough-for-hmac-1234";

    fn state() -> AppState {
        let config = Config::test_config(SECRET);
        AppState {
            db: Database::connect_lazy(&config).expect("lazy pool"),
            config,
            mailer: None,
            role_hierarchy: default_hierarchy(),
            #[cfg(feature = "templates")]
            templates: crate::template::TemplateEngine::empty(),
        }
    }

    fn token(secret: &str) -> String {
        JwtManager::new(secret, 60)
            .generate_full("a@example.com", "a@example.com", vec!["ROLE_ADMIN".into()], Some(7), None, None)
            .unwrap()
    }

    async fn guard(headers: &[(&str, String)]) -> Result<AuthGuard, AppError> {
        let mut request = Request::builder().uri("/admin");
        for (name, value) in headers {
            request = request.header(*name, value);
        }
        let (mut parts, ()) = request.body(()).unwrap().into_parts();
        AuthGuard::from_request_parts(&mut parts, &state()).await
    }

    /// A browser cannot send `Authorization`: before the session cookie, a server-rendered
    /// page behind a role could not be opened at all.
    #[tokio::test]
    async fn a_browser_session_cookie_authenticates_like_a_bearer_header() {
        let token = token(SECRET);

        let by_cookie = guard(&[("cookie", format!("csrf_token=x; {AUTH_COOKIE}={token}"))])
            .await
            .unwrap();
        assert_eq!(by_cookie.user_id(), Some(7));
        assert!(by_cookie.has_role("ROLE_USER"));

        let by_header = guard(&[("authorization", format!("Bearer {token}"))]).await.unwrap();
        assert_eq!(by_header.sub(), "a@example.com");
    }

    #[tokio::test]
    async fn a_cookie_is_verified_like_any_token() {
        let forged = token("another-secret-long-enough-for-hmac-5678");
        assert!(matches!(
            guard(&[("cookie", format!("{AUTH_COOKIE}={forged}"))]).await,
            Err(AppError::Unauthorized(_))
        ));
        assert!(matches!(guard(&[("cookie", format!("{AUTH_COOKIE}=garbage"))]).await, Err(AppError::Unauthorized(_))));
    }

    #[tokio::test]
    async fn no_credentials_and_a_broken_header_are_refused() {
        assert!(matches!(guard(&[]).await, Err(AppError::Unauthorized(m)) if m == "Authentication required"));

        // The header wins: a valid cookie does not rescue a malformed Authorization.
        let headers =
            [("authorization", "Basic abc".to_string()), ("cookie", format!("{AUTH_COOKIE}={}", token(SECRET)))];
        assert!(matches!(guard(&headers).await, Err(AppError::Unauthorized(m)) if m == "Invalid Authorization format"));
    }
}
