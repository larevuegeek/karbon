//! What every role hierarchy file goes through, whatever its syntax
//! ([`security_yaml`](super::security_yaml), [`security_toml`](super::security_toml)).
//!
//! Each reader only turns its syntax into [`RawEntry`] values; the rules live here, once:
//!
//! - role names match `ROLE_[A-Z0-9_]+`, 64 characters at most;
//! - a role declared twice, listed twice, inheriting itself or part of a cycle is an error;
//! - limits: 64 KiB, 1024 characters per line, 256 roles, 64 children per role;
//! - a file read at runtime must not be writable by every user of the server (Unix).

use std::collections::{HashMap, HashSet};
use std::path::Path;

pub(crate) const MAX_FILE_BYTES: usize = 64 * 1024;
pub(crate) const MAX_LINE_CHARS: usize = 1024;
const MAX_ROLES: usize = 256;
const MAX_CHILDREN: usize = 64;
const MAX_ROLE_LEN: usize = 64;

/// Why a role hierarchy file was refused.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RoleHierarchyError {
    #[error("cannot read {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("{path} is writable by every user of the server: anyone could grant themselves a role")]
    WorldWritable { path: String },

    #[error("no `security.role_hierarchy` section")]
    MissingSection,

    #[error("line {line}: {message}")]
    Invalid { line: usize, message: String },

    #[error("{0}")]
    TooLarge(String),

    #[error("cycle in the role hierarchy: {0}")]
    Cycle(String),

    #[error("both {first} and {second} exist: keep only one")]
    Ambiguous {
        first: &'static str,
        second: &'static str,
    },
}

/// One `ROLE_X: [...]` entry once checked, in file order.
pub(crate) type Entry = (String, Vec<String>);

/// One entry as a reader found it, with line numbers for the error messages.
pub(crate) struct RawEntry {
    pub role: String,
    pub line: usize,
    pub children: Vec<(String, usize)>,
}

/// Reads a hierarchy file, refusing oversized and (on Unix) world-writable files.
pub(crate) fn read_file(path: &Path) -> Result<String, RoleHierarchyError> {
    let io_error = |source| RoleHierarchyError::Io {
        path: path.display().to_string(),
        source,
    };

    let metadata = std::fs::metadata(path).map_err(io_error)?;

    if metadata.len() > MAX_FILE_BYTES as u64 {
        return Err(too_large_file());
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        if metadata.permissions().mode() & 0o002 != 0 {
            return Err(RoleHierarchyError::WorldWritable {
                path: path.display().to_string(),
            });
        }
    }

    std::fs::read_to_string(path).map_err(io_error)
}

/// Size limits, checked before any parsing.
pub(crate) fn check_source(source: &str) -> Result<(), RoleHierarchyError> {
    if source.len() > MAX_FILE_BYTES {
        return Err(too_large_file());
    }

    for (index, line) in source.split('\n').enumerate() {
        if line.chars().count() > MAX_LINE_CHARS {
            return Err(invalid(
                index + 1,
                format!("line longer than {MAX_LINE_CHARS} characters"),
            ));
        }
    }

    Ok(())
}

pub(crate) fn is_valid_role(name: &str) -> bool {
    name.len() > "ROLE_".len()
        && name.len() <= MAX_ROLE_LEN
        && name.starts_with("ROLE_")
        && name
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
}

pub(crate) fn invalid_role_rule() -> String {
    format!(
        "invalid role name: expected `ROLE_` then A-Z, 0-9 or `_` ({MAX_ROLE_LEN} characters max)"
    )
}

/// The checks no syntax can make: duplicates, self-inheritance, limits, cycles.
pub(crate) fn validate(raw: Vec<RawEntry>) -> Result<Vec<Entry>, RoleHierarchyError> {
    let mut declared: HashMap<&str, usize> = HashMap::new();

    // 1. Each role once, never its own child, each child once.
    for entry in &raw {
        if let Some(first_line) = declared.insert(&entry.role, entry.line) {
            return Err(invalid(
                entry.line,
                format!("`{}` is already declared on line {first_line}", entry.role),
            ));
        }

        let mut seen = HashSet::new();

        for (child, line) in &entry.children {
            if child == &entry.role {
                return Err(invalid(*line, format!("`{child}` cannot inherit itself")));
            }

            if !seen.insert(child.as_str()) {
                return Err(invalid(
                    *line,
                    format!("`{child}` is listed twice under `{}`", entry.role),
                ));
            }
        }
    }

    let entries: Vec<Entry> = raw
        .into_iter()
        .map(|entry| {
            let children = entry.children.into_iter().map(|(child, _)| child).collect();
            (entry.role, children)
        })
        .collect();

    // 2. Limits, then cycles.
    check_limits(&entries)?;
    check_cycles(&entries)?;

    Ok(entries)
}

fn check_limits(entries: &[Entry]) -> Result<(), RoleHierarchyError> {
    if entries.len() > MAX_ROLES {
        return Err(RoleHierarchyError::TooLarge(format!(
            "more than {MAX_ROLES} roles declared"
        )));
    }

    if let Some((role, _)) = entries
        .iter()
        .find(|(_, children)| children.len() > MAX_CHILDREN)
    {
        return Err(RoleHierarchyError::TooLarge(format!(
            "`{role}` lists more than {MAX_CHILDREN} roles"
        )));
    }

    Ok(())
}

/// A cycle makes every role in it equal to the others: almost certainly a mistake, and a
/// quiet way to hand a low role the rights of a high one.
fn check_cycles(entries: &[Entry]) -> Result<(), RoleHierarchyError> {
    let graph: HashMap<&str, &[String]> = entries
        .iter()
        .map(|(role, children)| (role.as_str(), children.as_slice()))
        .collect();

    let mut done: HashSet<&str> = HashSet::new();

    for (role, _) in entries {
        let mut path = Vec::new();
        visit(role, &graph, &mut path, &mut done)?;
    }

    Ok(())
}

fn visit<'a>(
    role: &'a str,
    graph: &HashMap<&'a str, &'a [String]>,
    path: &mut Vec<&'a str>,
    done: &mut HashSet<&'a str>,
) -> Result<(), RoleHierarchyError> {
    if let Some(start) = path.iter().position(|r| *r == role) {
        let mut cycle = path[start..].to_vec();
        cycle.push(role);

        return Err(RoleHierarchyError::Cycle(cycle.join(" → ")));
    }

    if done.contains(role) {
        return Ok(());
    }

    path.push(role);

    for child in graph.get(role).copied().unwrap_or_default() {
        visit(child, graph, path, done)?;
    }

    path.pop();
    done.insert(role);

    Ok(())
}

pub(crate) fn invalid(line: usize, message: impl Into<String>) -> RoleHierarchyError {
    RoleHierarchyError::Invalid {
        line,
        message: message.into(),
    }
}

fn too_large_file() -> RoleHierarchyError {
    RoleHierarchyError::TooLarge(format!("file larger than {} KiB", MAX_FILE_BYTES / 1024))
}

/// Offending text, escaped and truncated: error messages end up in logs.
pub(crate) fn excerpt(text: &str) -> String {
    let short: String = text.chars().take(40).collect();
    let escaped = short.escape_debug().to_string();

    if text.chars().count() > 40 {
        format!("{escaped}…")
    } else {
        escaped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(role: &str, line: usize, children: &[&str]) -> RawEntry {
        RawEntry {
            role: role.to_string(),
            line,
            children: children.iter().map(|c| (c.to_string(), line)).collect(),
        }
    }

    fn refused(entries: Vec<RawEntry>) -> String {
        validate(entries).expect_err("must be refused").to_string()
    }

    #[test]
    fn duplicates_self_inheritance_and_cycles_are_refused() {
        assert!(
            refused(vec![
                raw("ROLE_A", 3, &[]),
                raw("ROLE_A", 4, &["ROLE_USER"])
            ])
            .contains("already declared on line 3")
        );
        assert!(refused(vec![raw("ROLE_A", 3, &["ROLE_B", "ROLE_B"])]).contains("listed twice"));
        assert!(refused(vec![raw("ROLE_A", 3, &["ROLE_A"])]).contains("cannot inherit itself"));

        let message = refused(vec![
            raw("ROLE_USER", 3, &["ROLE_ADMIN"]),
            raw("ROLE_ADMIN", 4, &["ROLE_EMPLOYEE"]),
            raw("ROLE_EMPLOYEE", 5, &["ROLE_USER"]),
        ]);
        assert!(
            message.contains("ROLE_USER → ROLE_ADMIN → ROLE_EMPLOYEE → ROLE_USER"),
            "{message}"
        );
    }

    #[test]
    fn role_names_are_checked() {
        for name in ["ROLE_ADMIN", "ROLE_2FA", "ROLE_A_B"] {
            assert!(is_valid_role(name), "{name}");
        }

        let too_long = format!("ROLE_{}", "A".repeat(60));

        for name in [
            "role_admin",
            "ADMIN",
            "ROLE_",
            "ROLE_A-B",
            "ROLE_É",
            "ROLE_A B",
            &too_long,
        ] {
            assert!(!is_valid_role(name), "{name}");
        }
    }

    #[test]
    fn size_limits_are_enforced() {
        assert!(
            check_source(&"#".repeat(MAX_FILE_BYTES + 1))
                .unwrap_err()
                .to_string()
                .contains("KiB")
        );
        assert!(
            check_source(&format!("# {}\n", "x".repeat(MAX_LINE_CHARS)))
                .unwrap_err()
                .to_string()
                .contains("longer than")
        );

        let children: Vec<String> = (0..=MAX_CHILDREN).map(|i| format!("ROLE_C{i}")).collect();
        let children: Vec<&str> = children.iter().map(String::as_str).collect();
        assert!(refused(vec![raw("ROLE_A", 3, &children)]).contains("more than 64 roles"));
    }

    #[test]
    fn excerpts_escape_control_characters() {
        assert!(!excerpt("ROLE_B\u{1b}[31m").contains('\u{1b}'));
        assert!(excerpt(&"A".repeat(50)).ends_with('…'));
    }

    #[test]
    fn a_missing_file_is_an_io_error() {
        assert!(matches!(
            read_file(Path::new("does/not/exist.yaml")),
            Err(RoleHierarchyError::Io { .. })
        ));
    }
}
