pub mod auth_cookie;
mod crypto;
pub(crate) mod firewall;
mod guard;
mod jwt;
mod password;
mod role_file;
mod role_hierarchy;
mod secret;
mod security_toml;
mod security_yaml;
mod timing;

pub use auth_cookie::{
    AUTH_COOKIE, REFRESH_COOKIE, clear_refresh_cookie, local_redirect, login_cookie, logout_cookie, refresh_cookie,
    refresh_token_from,
};
pub use crypto::Crypto;
pub use firewall::{AccessControl, AccessControlBuilder};
pub use guard::{AuthGuard, require_role};
pub use jwt::{Claims, JwtManager, MIN_JWT_SECRET_LEN, is_placeholder_secret, is_weak_secret};
pub use password::Password;
pub use role_file::RoleHierarchyError;
pub use role_hierarchy::{RoleHierarchy, SECURITY_FILE, SECURITY_TOML_FILE, active_hierarchy, default_hierarchy};
pub(crate) use role_hierarchy::{activate_hierarchy, resolve_app_hierarchy};
pub use secret::Secret;
pub use timing::anti_timing_delay;
