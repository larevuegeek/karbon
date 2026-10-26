use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::OnceLock;

use super::security_yaml::{self, RoleHierarchyError};

/// File [`App`](crate::http::App) reads at startup, relative to the working directory.
pub const SECURITY_FILE: &str = "config/security.yaml";

/// Symfony-style role hierarchy.
///
/// Define which roles inherit from which others:
/// ```
/// use karbon_framework::security::RoleHierarchy;
///
/// let hierarchy = RoleHierarchy::new(&[
///     ("ROLE_SUPER_ADMIN", &["ROLE_ADMIN"]),
///     ("ROLE_ADMIN", &["ROLE_REDACTEUR"]),
///     ("ROLE_REDACTEUR", &["ROLE_USER"]),
/// ]);
///
/// // A user with ROLE_SUPER_ADMIN effectively has all roles
/// let resolved = hierarchy.resolve(&["ROLE_SUPER_ADMIN".to_string()]);
/// assert!(resolved.contains("ROLE_ADMIN"));
/// assert!(resolved.contains("ROLE_REDACTEUR"));
/// assert!(resolved.contains("ROLE_USER"));
/// ```
#[derive(Debug, Clone)]
pub struct RoleHierarchy {
    map: HashMap<String, Vec<String>>,
}

impl RoleHierarchy {
    /// Build from a list of (role, children) pairs.
    /// Each entry means: `role` inherits all `children` roles (and their children, transitively).
    pub fn new(entries: &[(&str, &[&str])]) -> Self {
        let map: HashMap<String, Vec<String>> = entries
            .iter()
            .map(|(role, children)| {
                (
                    role.to_string(),
                    children.iter().map(|c| c.to_string()).collect(),
                )
            })
            .collect();

        Self { map }
    }

    /// Build from the `security.role_hierarchy` section of a Symfony-style `security.yaml`.
    ///
    /// Embed the file to keep it out of reach of the server (`include_str!`), the hierarchy
    /// is then fixed at build time:
    ///
    /// ```ignore
    /// let hierarchy = RoleHierarchy::from_yaml(include_str!("../config/security.yaml"))?;
    /// ```
    ///
    /// Only a strict subset of YAML is read, see [`RoleHierarchyError`] for what is refused:
    /// no tags, anchors, aliases or keys other than `security.role_hierarchy`; role names
    /// `ROLE_[A-Z0-9_]+`; no duplicate, self-inheriting or cyclic role.
    pub fn from_yaml(source: &str) -> Result<Self, RoleHierarchyError> {
        let map = security_yaml::parse(source)?.into_iter().collect();

        Ok(Self { map })
    }

    /// Same as [`from_yaml`](Self::from_yaml), reading the file at runtime. Refuses a file over
    /// 64 KiB and, on Unix, a file every user of the server can write.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, RoleHierarchyError> {
        Self::from_yaml(&security_yaml::read_file(path.as_ref())?)
    }

    /// Resolve a set of user roles into the full set of effective roles
    /// (including all inherited roles, transitively).
    pub fn resolve(&self, user_roles: &[String]) -> HashSet<String> {
        let mut resolved = HashSet::new();
        for role in user_roles {
            self.resolve_recursive(role, &mut resolved);
        }
        resolved
    }

    /// Check if a user with the given roles effectively has `required_role`
    /// (directly or via hierarchy inheritance).
    pub fn has_role(&self, user_roles: &[String], required_role: &str) -> bool {
        // Fast path: direct match
        if user_roles.iter().any(|r| r == required_role) {
            return true;
        }
        // Slow path: resolve hierarchy
        self.resolve(user_roles).contains(required_role)
    }

    fn resolve_recursive(&self, role: &str, resolved: &mut HashSet<String>) {
        if !resolved.insert(role.to_string()) {
            return; // Already visited — avoid infinite loops
        }
        if let Some(children) = self.map.get(role) {
            for child in children {
                self.resolve_recursive(child, resolved);
            }
        }
    }
}

/// Default role hierarchy for LaRevueGeek. Still what an app runs with when it neither sets a
/// hierarchy nor ships `config/security.yaml`.
pub fn default_hierarchy() -> RoleHierarchy {
    RoleHierarchy::new(&[
        ("ROLE_SUPER_ADMIN", &["ROLE_ADMIN"]),
        ("ROLE_ADMIN", &["ROLE_REDACTEUR", "ROLE_MODERATEUR"]),
        ("ROLE_REDACTEUR", &["ROLE_USER"]),
        ("ROLE_MODERATEUR", &["ROLE_USER"]),
    ])
}

static ACTIVE: OnceLock<RoleHierarchy> = OnceLock::new();

/// Hierarchy the running app resolved at startup, or [`default_hierarchy`] outside an app.
pub fn active_hierarchy() -> RoleHierarchy {
    ACTIVE.get().cloned().unwrap_or_else(default_hierarchy)
}

/// Remembers the app's hierarchy for code that has no `AppState` at hand
/// ([`AuthGuard::from_claims`](crate::security::AuthGuard::from_claims)). First call wins.
pub(crate) fn activate_hierarchy(hierarchy: RoleHierarchy) {
    let _ = ACTIVE.set(hierarchy);
}

/// Hierarchy an app runs with, and where it came from (for the startup log):
///
/// 1. the one set in code with [`App::role_hierarchy`](crate::http::App::role_hierarchy);
/// 2. `config/security.yaml`, when the file exists and has a `role_hierarchy` section;
/// 3. [`default_hierarchy`], as before this file existed.
///
/// A file that exists but is invalid is an error, never a silent fallback to step 3.
pub(crate) fn resolve_app_hierarchy(
    explicit: Option<RoleHierarchy>,
) -> Result<(RoleHierarchy, &'static str), RoleHierarchyError> {
    if let Some(hierarchy) = explicit {
        return Ok((hierarchy, "set by the application"));
    }

    let path = Path::new(SECURITY_FILE);
    let exists = path.try_exists().map_err(|source| RoleHierarchyError::Io {
        path: SECURITY_FILE.to_string(),
        source,
    })?;

    if exists {
        match RoleHierarchy::from_file(path) {
            Ok(hierarchy) => return Ok((hierarchy, SECURITY_FILE)),
            Err(RoleHierarchyError::MissingSection) => {}
            Err(e) => return Err(e),
        }
    }

    Ok((default_hierarchy(), "default"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn super_admin_has_all_roles() {
        let h = default_hierarchy();
        let roles = vec!["ROLE_SUPER_ADMIN".to_string()];
        assert!(h.has_role(&roles, "ROLE_SUPER_ADMIN"));
        assert!(h.has_role(&roles, "ROLE_ADMIN"));
        assert!(h.has_role(&roles, "ROLE_REDACTEUR"));
        assert!(h.has_role(&roles, "ROLE_MODERATEUR"));
        assert!(h.has_role(&roles, "ROLE_USER"));
    }

    #[test]
    fn admin_does_not_have_super_admin() {
        let h = default_hierarchy();
        let roles = vec!["ROLE_ADMIN".to_string()];
        assert!(h.has_role(&roles, "ROLE_ADMIN"));
        assert!(h.has_role(&roles, "ROLE_REDACTEUR"));
        assert!(h.has_role(&roles, "ROLE_USER"));
        assert!(!h.has_role(&roles, "ROLE_SUPER_ADMIN"));
    }

    #[test]
    fn from_yaml_resolves_transitively() {
        let h = RoleHierarchy::from_yaml(
            "security:\n  role_hierarchy:\n    ROLE_SUPER_ADMIN: [ROLE_ADMIN, ROLE_ALLOWED_TO_SWITCH]\n    ROLE_ADMIN: [ROLE_EMPLOYEE]\n    ROLE_EMPLOYEE: [ROLE_USER]\n",
        )
        .unwrap();
        let roles = vec!["ROLE_SUPER_ADMIN".to_string()];

        assert!(h.has_role(&roles, "ROLE_USER"));
        assert!(h.has_role(&roles, "ROLE_ALLOWED_TO_SWITCH"));
        assert!(!h.has_role(&["ROLE_ADMIN".to_string()], "ROLE_SUPER_ADMIN"));
    }

    /// An app that sets nothing and ships no `config/security.yaml` keeps the hierarchy it had.
    #[test]
    fn without_file_or_explicit_hierarchy_the_default_applies() {
        assert!(!Path::new(SECURITY_FILE).exists());

        let (h, source) = resolve_app_hierarchy(None).unwrap();

        assert_eq!(source, "default");
        assert!(h.has_role(&["ROLE_ADMIN".to_string()], "ROLE_REDACTEUR"));
    }

    #[test]
    fn a_hierarchy_set_in_code_wins() {
        let explicit = RoleHierarchy::new(&[("ROLE_BOSS", &["ROLE_USER"])]);

        let (h, source) = resolve_app_hierarchy(Some(explicit)).unwrap();

        assert_eq!(source, "set by the application");
        assert!(h.has_role(&["ROLE_BOSS".to_string()], "ROLE_USER"));
        assert!(!h.has_role(&["ROLE_ADMIN".to_string()], "ROLE_REDACTEUR"));
    }

    #[test]
    fn basic_user_only_has_user() {
        let h = default_hierarchy();
        let roles = vec!["ROLE_USER".to_string()];
        assert!(h.has_role(&roles, "ROLE_USER"));
        assert!(!h.has_role(&roles, "ROLE_ADMIN"));
        assert!(!h.has_role(&roles, "ROLE_SUPER_ADMIN"));
    }
}
