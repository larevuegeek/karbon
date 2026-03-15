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
            Err(AppError::Forbidden(format!(
                "Role '{}' required",
                role
            )))
        }
    }

    /// Get the user ID
    pub fn user_id(&self) -> &str {
        &self.claims.sub
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
        let auth_header = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| AppError::Unauthorized("Missing Authorization header".to_string()))?;

        let token = auth_header
            .strip_prefix("Bearer ")
            .ok_or_else(|| AppError::Unauthorized("Invalid Authorization format".to_string()))?;

        // Verify JWT
        let jwt = super::JwtManager::new(&state.config.jwt_secret, state.config.jwt_expiration);
        let claims = jwt.verify(token).map_err(|_| {
            AppError::Unauthorized("Invalid or expired token".to_string())
        })?;

        Ok(AuthGuard {
            claims,
            role_hierarchy: state.role_hierarchy.clone(),
        })
    }
}

/// Middleware pour protéger un groupe de routes par ROLE_ADMIN.
pub async fn require_admin(
    auth: AuthGuard,
    request: Request,
    next: Next,
) -> Result<Response, AppError> {
    auth.require_role("ROLE_ADMIN")?;
    Ok(next.run(request).await)
}

/// Middleware pour protéger un groupe de routes par ROLE_REDACTEUR (ou supérieur).
pub async fn require_redacteur(
    auth: AuthGuard,
    request: Request,
    next: Next,
) -> Result<Response, AppError> {
    auth.require_role("ROLE_REDACTEUR")?;
    Ok(next.run(request).await)
}
