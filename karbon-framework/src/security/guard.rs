use std::future::Future;
use std::pin::Pin;

use axum::{
    extract::{FromRequestParts, Request},
    http::{header::AUTHORIZATION, request::Parts},
    middleware::Next,
    response::Response,
};

use crate::error::AppError;
use crate::http::AppState;

use super::{Claims, RoleHierarchy};

/// Extractor: pulls the authenticated user from the request
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
    /// Build an AuthGuard from existing Claims using the **default** role hierarchy.
    ///
    /// ⚠️ If your app configures a custom `RoleHierarchy`, use
    /// [`from_claims_with`](Self::from_claims_with) instead — otherwise `has_role` /
    /// `require_role` on this guard resolve against the default hierarchy, not yours,
    /// which can produce wrong authorization decisions.
    pub fn from_claims(claims: Claims) -> Self {
        Self {
            claims,
            role_hierarchy: crate::security::default_hierarchy(),
        }
    }

    /// Build an AuthGuard from existing Claims with an explicit role hierarchy (matches the
    /// app's configured `state.role_hierarchy`). Prefer this over [`from_claims`](Self::from_claims).
    pub fn from_claims_with(claims: Claims, role_hierarchy: RoleHierarchy) -> Self {
        Self {
            claims,
            role_hierarchy,
        }
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

    /// Get the username
    pub fn username(&self) -> &str {
        &self.claims.username
    }
}

impl FromRequestParts<AppState> for AuthGuard {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        // Extract Bearer token from Authorization header
        let auth_header: &str = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| AppError::Unauthorized("Missing Authorization header".to_string()))?;

        let token: &str = auth_header
            .strip_prefix("Bearer ")
            .ok_or_else(|| AppError::Unauthorized("Invalid Authorization format".to_string()))?;

        // Verify JWT
        let jwt: super::JwtManager =
            super::JwtManager::new(&state.config.jwt_secret, state.config.jwt_expiration);
        let claims: Claims = jwt
            .verify(token)
            .map_err(|_| AppError::Unauthorized("Invalid or expired token".to_string()))?;

        Ok(AuthGuard {
            claims,
            role_hierarchy: state.role_hierarchy.clone(),
        })
    }
}

/// Middleware factory pour protéger un groupe de routes par rôle.
///
/// ```ignore
/// router.layer(middleware::from_fn(require_role("ROLE_ADMIN")))
/// router.layer(middleware::from_fn(require_role("ROLE_REDACTEUR")))
/// ```
#[allow(clippy::type_complexity)]
pub fn require_role(
    role: &'static str,
) -> impl Fn(
    AuthGuard,
    Request,
    Next,
) -> Pin<Box<dyn Future<Output = Result<Response, AppError>> + Send>>
+ Clone {
    move |auth: AuthGuard, request: Request, next: Next| {
        Box::pin(async move {
            auth.require_role(role)?;
            Ok(next.run(request).await)
        })
    }
}
