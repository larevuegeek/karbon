use std::collections::{HashMap, HashSet};

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

/// Default role hierarchy for LaRevueGeek
pub fn default_hierarchy() -> RoleHierarchy {
    RoleHierarchy::new(&[
        ("ROLE_SUPER_ADMIN", &["ROLE_ADMIN"]),
        ("ROLE_ADMIN", &["ROLE_REDACTEUR", "ROLE_MODERATEUR"]),
        ("ROLE_REDACTEUR", &["ROLE_USER"]),
        ("ROLE_MODERATEUR", &["ROLE_USER"]),
    ])
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
    fn basic_user_only_has_user() {
        let h = default_hierarchy();
        let roles = vec!["ROLE_USER".to_string()];
        assert!(h.has_role(&roles, "ROLE_USER"));
        assert!(!h.has_role(&roles, "ROLE_ADMIN"));
        assert!(!h.has_role(&roles, "ROLE_SUPER_ADMIN"));
    }
}
