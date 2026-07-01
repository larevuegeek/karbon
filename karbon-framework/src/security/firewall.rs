//! Symfony-style **access control** (firewall): declarative URL-pattern → roles rules,
//! enforced centrally as a middleware before your handlers run.
//!
//! Rules are checked **top to bottom; the first matching rule wins** (like Symfony's
//! `access_control`). A matched rule either allows the request (`public`), requires
//! authentication with no specific role (empty roles), or requires **one of** the listed
//! roles (resolved through the app's [`RoleHierarchy`](crate::security::RoleHierarchy)).
//!
//! ```ignore
//! use karbon::security::AccessControl;
//!
//! let firewall = AccessControl::new()
//!     .public("^/login")
//!     .public("^/health")
//!     .rule("^/admin", ["ROLE_ADMIN"])       // /admin* → needs ROLE_ADMIN
//!     .rule("^/api/account", ["ROLE_USER"])  // needs ROLE_USER (or higher)
//!     .default_deny(false);                  // unmatched paths → allowed (default)
//!
//! App::new().router(base).firewall(firewall).serve().await?;
//! ```
//!
//! Enforcement: no match + `default_deny(false)` → allow; no match + `default_deny(true)`
//! → **403**; matched public → allow; matched but unauthenticated → **401**; matched but
//! missing role → **403**.

use std::sync::Arc;

use axum::{
    extract::{FromRequestParts, Request},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::http::AppState;
use crate::security::AuthGuard;

struct Rule {
    re: regex::Regex,
    roles: Vec<String>,
    public: bool,
}

/// A firewall: an ordered list of URL-pattern access rules.
#[derive(Default, Clone)]
pub struct AccessControl {
    rules: Arc<Vec<Rule>>,
    default_deny: bool,
}

// Builder state is a plain Vec; frozen into an Arc when wired.
#[derive(Default)]
struct Builder {
    rules: Vec<Rule>,
    default_deny: bool,
}

impl AccessControl {
    /// Start building a firewall. Returns a [builder](AccessControlBuilder).
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> AccessControlBuilder {
        AccessControlBuilder(Builder::default())
    }

    fn decision(&self, path: &str) -> Decision<'_> {
        match self.rules.iter().find(|r| r.re.is_match(path)) {
            None => {
                if self.default_deny {
                    Decision::Deny
                } else {
                    Decision::Allow
                }
            }
            Some(r) if r.public => Decision::Allow,
            Some(r) => Decision::Require(&r.roles),
        }
    }
}

enum Decision<'a> {
    Allow,
    Deny,
    /// Requires authentication; if the slice is non-empty, one of these roles too.
    Require(&'a [String]),
}

/// Fluent builder for [`AccessControl`].
pub struct AccessControlBuilder(Builder);

impl AccessControlBuilder {
    /// Require **one of** `roles` (empty = any authenticated user) for paths matching the
    /// regex `pattern`. Panics on an invalid pattern so a broken rule fails fast at startup
    /// rather than silently leaving the area unprotected.
    pub fn rule<I, S>(mut self, pattern: &str, roles: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.0.rules.push(Rule {
            re: compile(pattern),
            roles: roles.into_iter().map(Into::into).collect(),
            public: false,
        });
        self
    }

    /// Allow paths matching `pattern` without authentication (e.g. login, health, assets).
    pub fn public(mut self, pattern: &str) -> Self {
        self.0.rules.push(Rule {
            re: compile(pattern),
            roles: Vec::new(),
            public: true,
        });
        self
    }

    /// Whether a request matching **no** rule is denied (`true` → 403) or allowed
    /// (`false`, default).
    pub fn default_deny(mut self, deny: bool) -> Self {
        self.0.default_deny = deny;
        self
    }

    /// Freeze the builder into an [`AccessControl`].
    pub fn build(self) -> AccessControl {
        AccessControl {
            rules: Arc::new(self.0.rules),
            default_deny: self.0.default_deny,
        }
    }
}

// Allow passing a builder directly to `App::firewall(...)`.
impl From<AccessControlBuilder> for AccessControl {
    fn from(b: AccessControlBuilder) -> Self {
        b.build()
    }
}

fn compile(pattern: &str) -> regex::Regex {
    regex::Regex::new(pattern)
        .unwrap_or_else(|e| panic!("invalid firewall pattern `{pattern}`: {e}"))
}

/// Middleware that enforces an [`AccessControl`] against the request path. Wired by
/// `App::firewall(...)`; not usually called directly.
pub async fn enforce(
    ac: Arc<AccessControl>,
    state: AppState,
    req: Request,
    next: Next,
) -> Response {
    let roles = match ac.decision(req.uri().path()) {
        Decision::Allow => return next.run(req).await,
        Decision::Deny => return (StatusCode::FORBIDDEN, "Forbidden").into_response(),
        Decision::Require(roles) => roles.to_vec(),
    };

    // Authenticate via the same path as AuthGuard (JWT + role hierarchy).
    let (mut parts, body) = req.into_parts();
    match AuthGuard::from_request_parts(&mut parts, &state).await {
        Ok(guard) => {
            if roles.is_empty() || roles.iter().any(|r| guard.has_role(r)) {
                next.run(Request::from_parts(parts, body)).await
            } else {
                (StatusCode::FORBIDDEN, "Forbidden").into_response()
            }
        }
        Err(_) => (StatusCode::UNAUTHORIZED, "Unauthorized").into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ac() -> AccessControl {
        AccessControl::new()
            .public("^/login")
            .rule("^/admin", ["ROLE_ADMIN"])
            .rule("^/api/account", ["ROLE_USER"])
            .build()
    }

    #[test]
    fn first_match_wins_and_public_is_open() {
        assert!(matches!(ac().decision("/login"), Decision::Allow));
    }

    #[test]
    fn admin_requires_admin_role() {
        match ac().decision("/admin/posts") {
            Decision::Require(roles) => assert_eq!(roles, ["ROLE_ADMIN"]),
            _ => panic!("expected Require"),
        }
    }

    #[test]
    fn unmatched_defaults_to_allow() {
        assert!(matches!(ac().decision("/public/page"), Decision::Allow));
    }

    #[test]
    fn unmatched_can_default_deny() {
        let fw = AccessControl::new()
            .rule("^/admin", ["ROLE_ADMIN"])
            .default_deny(true)
            .build();
        assert!(matches!(fw.decision("/anything"), Decision::Deny));
    }

    #[test]
    fn empty_roles_means_authenticated_only() {
        let fw = AccessControl::new()
            .rule("^/members", Vec::<String>::new())
            .build();
        match fw.decision("/members/area") {
            Decision::Require(roles) => assert!(roles.is_empty()),
            _ => panic!("expected Require"),
        }
    }
}
