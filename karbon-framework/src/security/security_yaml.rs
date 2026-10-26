//! Strict reader for the `role_hierarchy` section of `config/security.yaml`.
//!
//! This is deliberately **not** a general YAML parser. YAML accepts tags (`!!type`), anchors
//! and aliases (`&a` / `*a`, the "billion laughs" expansion), merge keys, multi-line scalars
//! and several documents per file: nothing a role file needs, and everything a tampered file
//! needs to say something different from what a human reads. The grammar here is a whitelist
//! of the Symfony shape; anything else is refused with its line number.
//!
//! ```yaml
//! security:
//!     role_hierarchy:
//!         ROLE_ADMIN:       [ROLE_EDITOR, ROLE_USER]
//!         ROLE_SUPER_ADMIN: ROLE_ADMIN
//!         ROLE_EDITOR:
//!             - ROLE_USER
//! ```
//!
//! - only the `security` → `role_hierarchy` keys: any other key is an error, never skipped;
//! - role names match `ROLE_[A-Z0-9_]+`, 64 characters at most, optionally quoted;
//! - a role declared twice, listed twice, inheriting itself or part of a cycle is an error;
//! - limits: 64 KiB, 1024 characters per line, 256 roles, 64 children per role;
//! - comments (whole line or trailing), blank lines, CRLF and a UTF-8 BOM are accepted.

use std::collections::{HashMap, HashSet};
use std::path::Path;

pub(crate) const MAX_FILE_BYTES: usize = 64 * 1024;
const MAX_LINE_CHARS: usize = 1024;
const MAX_ROLES: usize = 256;
const MAX_CHILDREN: usize = 64;
const MAX_ROLE_LEN: usize = 64;

/// Why a role hierarchy file was refused.
#[derive(Debug, thiserror::Error)]
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
}

/// One `ROLE_X: [...]` entry, in file order.
pub(crate) type Entry = (String, Vec<String>);

struct Line<'a> {
    number: usize,
    indent: usize,
    text: &'a str,
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

/// Parses `security.role_hierarchy` and checks the result (names, duplicates, limits, cycles).
pub(crate) fn parse(source: &str) -> Result<Vec<Entry>, RoleHierarchyError> {
    if source.len() > MAX_FILE_BYTES {
        return Err(too_large_file());
    }

    // 1. Meaningful lines only: comments and blank lines dropped, indentation measured.
    let lines = significant_lines(source.strip_prefix('\u{feff}').unwrap_or(source))?;

    // 2. `security:` then `role_hierarchy:`, nothing else.
    let Some(section_indent) = find_role_hierarchy(&lines)? else {
        return Err(RoleHierarchyError::MissingSection);
    };

    // 3. The entries, then the checks a parser alone cannot make.
    let entries = parse_entries(&lines[2..], section_indent)?;

    check_limits(&entries)?;
    check_cycles(&entries)?;

    Ok(entries)
}

fn significant_lines(source: &str) -> Result<Vec<Line<'_>>, RoleHierarchyError> {
    let mut lines = Vec::new();

    for (index, raw) in source.split('\n').enumerate() {
        let number = index + 1;
        let raw = raw.strip_suffix('\r').unwrap_or(raw);

        if raw.chars().count() > MAX_LINE_CHARS {
            return Err(invalid(
                number,
                format!("line longer than {MAX_LINE_CHARS} characters"),
            ));
        }

        let text = strip_comment(raw).trim_end();
        let content = text.trim_start();

        if content.is_empty() {
            continue;
        }

        let leading = &text[..text.len() - content.len()];

        if leading.contains('\t') {
            return Err(invalid(number, "tab in indentation: indent with spaces"));
        }

        lines.push(Line {
            number,
            indent: leading.len(),
            text: content,
        });
    }

    Ok(lines)
}

/// A `#` starts a comment at the beginning of a line or after whitespace. Role names cannot
/// contain `#` or spaces, so a cut inside a quoted value only leaves something invalid.
fn strip_comment(line: &str) -> &str {
    let bytes = line.as_bytes();

    for (i, &b) in bytes.iter().enumerate() {
        if b == b'#' && (i == 0 || bytes[i - 1] == b' ' || bytes[i - 1] == b'\t') {
            return &line[..i];
        }
    }

    line
}

/// Indentation of `role_hierarchy:` (its entries start on the third line), `None` when the
/// section is absent.
fn find_role_hierarchy(lines: &[Line]) -> Result<Option<usize>, RoleHierarchyError> {
    let Some(first) = lines.first() else {
        return Ok(None);
    };

    if first.indent != 0 || first.text != "security:" {
        return Err(invalid(
            first.number,
            unsupported(first.text, "only `security:` is allowed at the top level"),
        ));
    }

    let Some(second) = lines.get(1) else {
        return Ok(None);
    };

    if second.indent == 0 {
        return Err(invalid(
            second.number,
            unsupported(second.text, "`security:` can only appear once"),
        ));
    }

    if second.text != "role_hierarchy:" {
        return Err(invalid(
            second.number,
            unsupported(
                second.text,
                "only `role_hierarchy` is read under `security`",
            ),
        ));
    }

    Ok(Some(second.indent))
}

fn parse_entries(lines: &[Line], section_indent: usize) -> Result<Vec<Entry>, RoleHierarchyError> {
    let mut entries: Vec<Entry> = Vec::new();
    let mut declared: HashMap<String, usize> = HashMap::new();
    let mut entry_indent: Option<usize> = None;
    let mut i = 0;

    while i < lines.len() {
        let line = &lines[i];

        // 1. Back to a shallower level: another key under `security` or at the top level.
        if line.indent <= section_indent {
            return Err(invalid(
                line.number,
                unsupported(line.text, "only `security.role_hierarchy` is read"),
            ));
        }

        let indent = *entry_indent.get_or_insert(line.indent);

        if line.indent != indent {
            return Err(invalid(line.number, "inconsistent indentation"));
        }

        // 2. `ROLE_X: value`, or `ROLE_X:` followed by a block list.
        let (key, value) = split_entry(line)?;
        let role = parse_role(key, line.number)?;

        if let Some(first_line) = declared.insert(role.clone(), line.number) {
            return Err(invalid(
                line.number,
                format!("`{role}` is already declared on line {first_line}"),
            ));
        }

        let (children, consumed) = if value.is_empty() {
            parse_block_list(&lines[i + 1..], indent, line.number)?
        } else {
            (parse_inline_value(value, line.number)?, 0)
        };

        // 3. Children: unique, and never the role itself.
        let mut seen = HashSet::new();

        for (child, child_line) in &children {
            if child == &role {
                return Err(invalid(
                    *child_line,
                    format!("`{role}` cannot inherit itself"),
                ));
            }

            if !seen.insert(child.as_str()) {
                return Err(invalid(
                    *child_line,
                    format!("`{child}` is listed twice under `{role}`"),
                ));
            }
        }

        entries.push((role, children.into_iter().map(|(child, _)| child).collect()));
        i += 1 + consumed;
    }

    Ok(entries)
}

fn split_entry<'a>(line: &Line<'a>) -> Result<(&'a str, &'a str), RoleHierarchyError> {
    let Some(colon) = line.text.find(':') else {
        return Err(invalid(
            line.number,
            unsupported(line.text, "expected `ROLE_NAME: [ROLE_CHILD, …]`"),
        ));
    };

    let (key, rest) = (&line.text[..colon], &line.text[colon + 1..]);

    if !rest.is_empty() && !rest.starts_with(' ') {
        return Err(invalid(line.number, "expected a space after `:`"));
    }

    Ok((key.trim(), rest.trim()))
}

/// `[A, B]`, `[]` or a single role.
fn parse_inline_value(
    value: &str,
    line: usize,
) -> Result<Vec<(String, usize)>, RoleHierarchyError> {
    let Some(inner) = value.strip_prefix('[') else {
        return Ok(vec![(parse_role(value, line)?, line)]);
    };

    let Some(inner) = inner.strip_suffix(']') else {
        return Err(invalid(
            line,
            "a list opened with `[` must close with `]` on the same line",
        ));
    };

    let inner = inner.trim();

    if inner.is_empty() {
        return Ok(Vec::new());
    }

    let mut items: Vec<&str> = inner.split(',').map(str::trim).collect();

    // YAML tolerates one trailing comma: `[A, B,]`.
    if items.last() == Some(&"") {
        items.pop();
    }

    items
        .into_iter()
        .map(|item| {
            if item.is_empty() {
                return Err(invalid(line, "empty element in the list"));
            }

            Ok((parse_role(item, line)?, line))
        })
        .collect()
}

/// `- ROLE_X` lines under an entry. Returns the children and the number of lines consumed.
fn parse_block_list(
    lines: &[Line],
    entry_indent: usize,
    entry_line: usize,
) -> Result<(Vec<(String, usize)>, usize), RoleHierarchyError> {
    let mut children = Vec::new();
    let mut item_indent: Option<usize> = None;

    for line in lines {
        if line.indent < entry_indent || !line.text.starts_with('-') {
            break;
        }

        let indent = *item_indent.get_or_insert(line.indent);

        if line.indent != indent {
            return Err(invalid(line.number, "inconsistent indentation in the list"));
        }

        let Some(item) = line.text.strip_prefix("- ") else {
            return Err(invalid(line.number, "expected `- ROLE_NAME`"));
        };

        children.push((parse_role(item.trim(), line.number)?, line.number));
    }

    if children.is_empty() {
        return Err(invalid(
            entry_line,
            "no children: write `[]` for a role that inherits nothing",
        ));
    }

    let consumed = children.len();

    Ok((children, consumed))
}

fn parse_role(token: &str, line: usize) -> Result<String, RoleHierarchyError> {
    let unquoted = ['\'', '"']
        .iter()
        .find_map(|q| token.strip_prefix(*q).and_then(|t| t.strip_suffix(*q)))
        .filter(|_| token.len() >= 2)
        .unwrap_or(token);

    let valid = unquoted.len() > "ROLE_".len()
        && unquoted.len() <= MAX_ROLE_LEN
        && unquoted.starts_with("ROLE_")
        && unquoted
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_');

    if !valid {
        return Err(invalid(
            line,
            unsupported(
                token,
                &format!(
                    "invalid role name: expected `ROLE_` then A-Z, 0-9 or `_` ({MAX_ROLE_LEN} characters max)"
                ),
            ),
        ));
    }

    Ok(unquoted.to_string())
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

fn invalid(line: usize, message: impl Into<String>) -> RoleHierarchyError {
    RoleHierarchyError::Invalid {
        line,
        message: message.into(),
    }
}

fn too_large_file() -> RoleHierarchyError {
    RoleHierarchyError::TooLarge(format!("file larger than {} KiB", MAX_FILE_BYTES / 1024))
}

/// Names the YAML feature when it is a known one, and quotes the offending text escaped and
/// truncated: the message ends up in logs.
fn unsupported(text: &str, fallback: &str) -> String {
    let feature = match text.trim_start_matches("- ").chars().next() {
        Some('&') | Some('*') => Some("anchors and aliases are not supported"),
        Some('!') => Some("tags are not supported"),
        Some('{') => Some("flow mappings are not supported"),
        Some('|') | Some('>') => Some("block scalars are not supported"),
        Some('%') => Some("directives are not supported"),
        _ if text == "---" || text == "..." => {
            Some("only one YAML document is allowed, without `---`")
        }
        _ if text.starts_with("<<") => Some("merge keys are not supported"),
        _ => None,
    };

    format!(
        "{} (found `{}`)",
        feature.unwrap_or(fallback),
        excerpt(text)
    )
}

fn excerpt(text: &str) -> String {
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

    fn entries(source: &str) -> HashMap<String, Vec<String>> {
        parse(source).expect("valid file").into_iter().collect()
    }

    fn refused(source: &str) -> String {
        parse(source).expect_err("must be refused").to_string()
    }

    fn roles(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    /// The block of a Symfony `security.yaml`, pasted as is.
    #[test]
    fn reads_a_symfony_role_hierarchy() {
        let map = entries(
            "security:\n    role_hierarchy:\n        ROLE_EMPLOYEE:    [ROLE_USER]\n        ROLE_ADMIN:       [ROLE_EMPLOYEE, ROLE_USER]\n        ROLE_SUPER_ADMIN: [ROLE_USER, ROLE_ADMIN, ROLE_ALLOWED_TO_SWITCH]\n",
        );

        assert_eq!(map.len(), 3);
        assert_eq!(map["ROLE_ADMIN"], roles(&["ROLE_EMPLOYEE", "ROLE_USER"]));
        assert_eq!(
            map["ROLE_SUPER_ADMIN"],
            roles(&["ROLE_USER", "ROLE_ADMIN", "ROLE_ALLOWED_TO_SWITCH"])
        );
    }

    #[test]
    fn inline_list_single_value_and_block_lists_are_equivalent() {
        let map = entries(
            "security:\n  role_hierarchy:\n    ROLE_A: [ROLE_USER]\n    ROLE_B: ROLE_USER\n    ROLE_C:\n      - ROLE_USER\n    ROLE_D:\n    - ROLE_USER\n",
        );

        for role in ["ROLE_A", "ROLE_B", "ROLE_C", "ROLE_D"] {
            assert_eq!(map[role], roles(&["ROLE_USER"]), "{role}");
        }
    }

    #[test]
    fn accepts_comments_crlf_bom_quotes_trailing_comma_and_empty_lists() {
        let map = entries(
            "\u{feff}# Rôles de l'application\r\nsecurity: # section\r\n\r\n  role_hierarchy:\r\n    'ROLE_A': [\"ROLE_B\", 'ROLE_C',]  # fin\r\n    ROLE_B: []\r\n",
        );

        assert_eq!(map["ROLE_A"], roles(&["ROLE_B", "ROLE_C"]));
        assert!(map["ROLE_B"].is_empty());
    }

    #[test]
    fn a_file_without_the_section_is_reported_as_such() {
        for source in ["", "# rien\n\n", "security:\n"] {
            assert!(
                matches!(parse(source), Err(RoleHierarchyError::MissingSection)),
                "{source:?}"
            );
        }
    }

    #[test]
    fn keys_other_than_security_role_hierarchy_are_refused() {
        assert!(refused("parameters:\n  x: 1\n").contains("only `security:`"));
        assert!(refused("security:\n  providers:\n    x: y\n").contains("only `role_hierarchy`"));
        assert!(
            refused("security:\n  role_hierarchy:\n    ROLE_A: []\n  firewalls:\n    main: ~\n")
                .contains("only `security.role_hierarchy`")
        );
        assert!(refused("security:\n  role_hierarchy:\n    ROLE_A: []\nsecurity:\n  role_hierarchy:\n    ROLE_USER: [ROLE_A]\n").contains("line 4"));
    }

    /// What a general YAML parser would interpret, and this reader refuses.
    #[test]
    fn yaml_features_beyond_the_subset_are_refused() {
        let base = "security:\n  role_hierarchy:\n";

        let cases = [
            ("    ROLE_A: !!str ROLE_B\n", "tags"),
            ("    ROLE_A: &admin [ROLE_B]\n", "anchors and aliases"),
            ("    ROLE_A: *admin\n", "anchors and aliases"),
            ("    <<: *admin\n", "merge keys"),
            ("    ROLE_A: {ROLE_B: x}\n", "flow mappings"),
            ("    ROLE_A: |\n      ROLE_B\n", "block scalars"),
            ("    !!python/object: x\n", "tags"),
        ];

        for (body, expected) in cases {
            let message = refused(&format!("{base}{body}"));
            assert!(message.contains(expected), "{body:?} → {message}");
        }

        assert!(refused("---\nsecurity:\n").contains("one YAML document"));
        assert!(refused("%YAML 1.2\nsecurity:\n").contains("directives"));
    }

    #[test]
    fn role_names_are_validated() {
        let base = "security:\n  role_hierarchy:\n";
        let too_long = format!("ROLE_{}", "A".repeat(60));

        for name in [
            "role_admin",
            "ADMIN",
            "ROLE_",
            "ROLE_ADMIN;",
            "ROLE_A-B",
            "'ROLE_A",
            "ROLE_É",
            too_long.as_str(),
        ] {
            let message = refused(&format!("{base}    {name}: []\n"));
            assert!(message.contains("invalid role name"), "{name} → {message}");
        }
    }

    /// Text glued to a value (`;…`, `$(…)`) never becomes part of a role: the whole token is
    /// refused, and the offending text is quoted escaped in the message.
    #[test]
    fn injection_attempts_are_refused_and_escaped_in_the_message() {
        assert!(
            refused("security:\n  role_hierarchy:\n    ROLE_A: [ROLE_B;rm]\n")
                .contains("invalid role name")
        );
        assert!(
            refused("security:\n  role_hierarchy:\n    ROLE_A: [$(id)]\n")
                .contains("invalid role name")
        );

        let message = refused("security:\n  role_hierarchy:\n    ROLE_A: [ROLE_B\u{1b}[31m]\n");
        assert!(
            !message.contains('\u{1b}'),
            "control characters must be escaped: {message}"
        );
    }

    #[test]
    fn duplicates_self_inheritance_and_cycles_are_refused() {
        let base = "security:\n  role_hierarchy:\n";

        assert!(
            refused(&format!("{base}    ROLE_A: []\n    ROLE_A: [ROLE_USER]\n"))
                .contains("already declared on line 3")
        );
        assert!(refused(&format!("{base}    ROLE_A: [ROLE_B, ROLE_B]\n")).contains("listed twice"));
        assert!(
            refused(&format!("{base}    ROLE_A: [ROLE_A]\n")).contains("cannot inherit itself")
        );

        let message = refused(&format!(
            "{base}    ROLE_USER: [ROLE_ADMIN]\n    ROLE_ADMIN: [ROLE_EMPLOYEE]\n    ROLE_EMPLOYEE: [ROLE_USER]\n"
        ));
        assert!(
            message.contains("ROLE_USER → ROLE_ADMIN → ROLE_EMPLOYEE → ROLE_USER"),
            "{message}"
        );
    }

    #[test]
    fn malformed_entries_are_refused() {
        let base = "security:\n  role_hierarchy:\n";

        assert!(refused(&format!("{base}    ROLE_A: [ROLE_B\n")).contains("must close"));
        assert!(
            refused(&format!("{base}    ROLE_A: [ROLE_B,,ROLE_C]\n")).contains("empty element")
        );
        assert!(refused(&format!("{base}    ROLE_A:\n    ROLE_B: []\n")).contains("no children"));
        assert!(refused(&format!("{base}    ROLE_A:[ROLE_B]\n")).contains("space after"));
        assert!(
            refused(&format!("{base}    ROLE_A: []\n      ROLE_B: []\n"))
                .contains("inconsistent indentation")
        );
        assert!(
            refused(&format!("{base}    ROLE_A:\n      -ROLE_B\n"))
                .contains("expected `- ROLE_NAME`")
        );
        assert!(refused(&format!("{base}\tROLE_A: []\n")).contains("tab"));
    }

    #[test]
    fn size_limits_are_enforced() {
        assert!(refused(&"#".repeat(MAX_FILE_BYTES + 1)).contains("KiB"));
        assert!(refused(&format!("# {}\n", "x".repeat(MAX_LINE_CHARS))).contains("longer than"));

        let children: Vec<String> = (0..=MAX_CHILDREN).map(|i| format!("ROLE_C{i}")).collect();
        let message = refused(&format!(
            "security:\n  role_hierarchy:\n    ROLE_A: [{}]\n",
            children.join(", ")
        ));
        assert!(message.contains("more than 64 roles"), "{message}");
    }

    #[test]
    fn a_missing_file_is_an_io_error() {
        assert!(matches!(
            read_file(Path::new("does/not/exist.yaml")),
            Err(RoleHierarchyError::Io { .. })
        ));
    }
}
