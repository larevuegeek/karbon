//! Reader for the `role_hierarchy` section of `config/security.toml`.
//!
//! TOML has none of the YAML features a tampered file could hide behind (no anchors, aliases,
//! tags or guessed types), so the syntax is left to the `toml` crate, built without its
//! `unbounded` feature (nesting depth stays capped). The document is then walked as a
//! whitelist, like the YAML reader, with every key and value located to its line.
//!
//! ```toml
//! [security.role_hierarchy]
//! ROLE_ADMIN       = ["ROLE_EDITOR", "ROLE_USER"]
//! ROLE_SUPER_ADMIN = "ROLE_ADMIN"
//! ```
//!
//! Only the `security` → `role_hierarchy` tables are read: any other key is an error, never
//! skipped. Names, duplicates, cycles and limits: see [`super::role_file`].

use toml::Spanned;
use toml::de::{DeTable, DeValue};

use super::role_file::{
    Entry, RawEntry, RoleHierarchyError, check_source, excerpt, invalid, invalid_role_rule,
    is_valid_role, validate,
};

/// Parses `security.role_hierarchy` and checks the result.
pub(crate) fn parse(source: &str) -> Result<Vec<Entry>, RoleHierarchyError> {
    check_source(source)?;

    let source = source.strip_prefix('\u{feff}').unwrap_or(source);

    // 1. Syntax: the `toml` crate (duplicate keys, unclosed strings… are its errors).
    let document = DeTable::parse(source).map_err(|e| syntax_error(source, &e))?;

    // 2. `security` then `role_hierarchy`, nothing else.
    let Some(section) = find_role_hierarchy(source, document.get_ref())? else {
        return Err(RoleHierarchyError::MissingSection);
    };

    // 3. The entries in file order, then the shared checks.
    validate(parse_entries(source, section)?)
}

fn find_role_hierarchy<'a>(
    source: &str,
    document: &'a DeTable<'a>,
) -> Result<Option<&'a DeTable<'a>>, RoleHierarchyError> {
    let Some(security) = single_table(source, document, "security", "at the top level")? else {
        return Ok(None);
    };

    single_table(source, security, "role_hierarchy", "under `security`")
}

/// The table `name`, when `table` holds it and nothing else.
fn single_table<'a>(
    source: &str,
    table: &'a DeTable<'a>,
    name: &str,
    place: &str,
) -> Result<Option<&'a DeTable<'a>>, RoleHierarchyError> {
    let mut found = None;

    for (key, value) in table.iter() {
        if key.get_ref() != name {
            return Err(invalid(
                line_of(source, key.span().start),
                format!(
                    "only `{name}` is allowed {place} (found `{}`)",
                    excerpt(key.get_ref())
                ),
            ));
        }

        let DeValue::Table(inner) = value.get_ref() else {
            return Err(invalid(
                line_of(source, value.span().start),
                format!("`{name}` must be a table"),
            ));
        };

        found = Some(inner);
    }

    Ok(found)
}

fn parse_entries(source: &str, section: &DeTable) -> Result<Vec<RawEntry>, RoleHierarchyError> {
    // The map is ordered by key: back to file order, so errors point at the first culprit.
    let mut items: Vec<_> = section.iter().collect();
    items.sort_by_key(|(key, _)| key.span().start);

    items
        .into_iter()
        .map(|(key, value)| {
            let line = line_of(source, key.span().start);

            Ok(RawEntry {
                role: parse_role(key.get_ref(), line)?,
                line,
                children: parse_children(source, value)?,
            })
        })
        .collect()
}

/// `["ROLE_A", "ROLE_B"]`, `[]` or a single `"ROLE_A"`.
fn parse_children(
    source: &str,
    value: &Spanned<DeValue>,
) -> Result<Vec<(String, usize)>, RoleHierarchyError> {
    let line = line_of(source, value.span().start);

    match value.get_ref() {
        DeValue::String(role) => Ok(vec![(parse_role(role, line)?, line)]),

        DeValue::Array(items) => items
            .iter()
            .map(|item| {
                let line = line_of(source, item.span().start);

                let DeValue::String(role) = item.get_ref() else {
                    return Err(invalid(line, "expected a role name between quotes"));
                };

                Ok((parse_role(role, line)?, line))
            })
            .collect(),

        _ => Err(invalid(line, "expected `[\"ROLE_CHILD\", …]`")),
    }
}

fn parse_role(name: &str, line: usize) -> Result<String, RoleHierarchyError> {
    if !is_valid_role(name) {
        return Err(invalid(
            line,
            format!("{} (found `{}`)", invalid_role_rule(), excerpt(name)),
        ));
    }

    Ok(name.to_string())
}

fn syntax_error(source: &str, error: &toml::de::Error) -> RoleHierarchyError {
    let line = error.span().map_or(1, |span| line_of(source, span.start));

    invalid(line, format!("invalid TOML: {}", excerpt(error.message())))
}

fn line_of(source: &str, offset: usize) -> usize {
    let end = offset.min(source.len());

    source.as_bytes()[..end]
        .iter()
        .filter(|&&b| b == b'\n')
        .count()
        + 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn entries(source: &str) -> HashMap<String, Vec<String>> {
        parse(source).expect("valid file").into_iter().collect()
    }

    fn refused(source: &str) -> String {
        parse(source).expect_err("must be refused").to_string()
    }

    fn roles(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn reads_a_role_hierarchy() {
        let map = entries(
            "# Rôles\n[security.role_hierarchy]\nROLE_EMPLOYEE    = [\"ROLE_USER\"]\nROLE_ADMIN       = [\"ROLE_EMPLOYEE\", \"ROLE_USER\"]\nROLE_SUPER_ADMIN = [\"ROLE_USER\", \"ROLE_ADMIN\", \"ROLE_ALLOWED_TO_SWITCH\"]\n",
        );

        assert_eq!(map.len(), 3);
        assert_eq!(map["ROLE_ADMIN"], roles(&["ROLE_EMPLOYEE", "ROLE_USER"]));
        assert_eq!(
            map["ROLE_SUPER_ADMIN"],
            roles(&["ROLE_USER", "ROLE_ADMIN", "ROLE_ALLOWED_TO_SWITCH"])
        );
    }

    /// Same hierarchy as the YAML reader gets from the Symfony block.
    #[test]
    fn toml_and_yaml_give_the_same_hierarchy() {
        let toml = parse("[security.role_hierarchy]\nROLE_ADMIN = [\"ROLE_EMPLOYEE\"]\nROLE_EMPLOYEE = \"ROLE_USER\"\n").unwrap();
        let yaml = super::super::security_yaml::parse(
            "security:\n  role_hierarchy:\n    ROLE_ADMIN: [ROLE_EMPLOYEE]\n    ROLE_EMPLOYEE: ROLE_USER\n",
        )
        .unwrap();

        assert_eq!(toml, yaml);
    }

    #[test]
    fn accepts_every_table_spelling_crlf_bom_literal_strings_and_empty_lists() {
        let sources = [
            "[security.role_hierarchy]\nROLE_A = ['ROLE_B']\nROLE_B = []\n",
            "[security]\nrole_hierarchy = { ROLE_A = [\"ROLE_B\"], ROLE_B = [] }\n",
            "\u{feff}[security.role_hierarchy] # fin\r\n\"ROLE_A\" = [\r\n  \"ROLE_B\",\r\n]\r\nROLE_B = []\r\n",
        ];

        for source in sources {
            let map = entries(source);
            assert_eq!(map["ROLE_A"], roles(&["ROLE_B"]), "{source:?}");
            assert!(map["ROLE_B"].is_empty(), "{source:?}");
        }
    }

    #[test]
    fn a_file_without_the_section_is_reported_as_such() {
        for source in ["", "# rien\n", "[security]\n"] {
            assert!(
                matches!(parse(source), Err(RoleHierarchyError::MissingSection)),
                "{source:?}"
            );
        }
    }

    #[test]
    fn keys_other_than_security_role_hierarchy_are_refused() {
        assert!(refused("title = \"x\"\n").contains("line 1: only `security`"));
        assert!(
            refused("[security.role_hierarchy]\nROLE_A = []\n\n[security.providers]\nx = 1\n")
                .contains("line 4: only `role_hierarchy`")
        );
        assert!(refused("security = \"x\"\n").contains("must be a table"));
        assert!(refused("[security]\nrole_hierarchy = 1\n").contains("must be a table"));
    }

    #[test]
    fn values_must_be_role_names() {
        let base = "[security.role_hierarchy]\n";

        assert!(refused(&format!("{base}ROLE_A = 1\n")).contains("expected `["));
        assert!(refused(&format!("{base}ROLE_A = [1]\n")).contains("between quotes"));
        assert!(refused(&format!("{base}ROLE_A = [[\"ROLE_B\"]]\n")).contains("between quotes"));
        assert!(refused(&format!("{base}ROLE_A = {{ x = 1 }}\n")).contains("expected `["));
        assert!(refused(&format!("{base}ROLE_A.B = []\n")).contains("expected `["));
    }

    #[test]
    fn role_names_are_validated_and_escaped() {
        let base = "[security.role_hierarchy]\n";

        for key in ["role_admin", "\"ROLE_A;rm\"", "\"$(id)\"", "\"ROLE_A B\""] {
            let message = refused(&format!("{base}{key} = []\n"));
            assert!(
                message.contains("line 2: invalid role name"),
                "{key} → {message}"
            );
        }

        let message = refused(&format!("{base}ROLE_A = [\"ROLE_B\\u001b[31m\"]\n"));
        assert!(message.contains("invalid role name"), "{message}");
        assert!(!message.contains('\u{1b}'), "{message}");
    }

    #[test]
    fn syntax_errors_and_duplicate_keys_carry_their_line() {
        let base = "[security.role_hierarchy]\n";

        assert!(refused(&format!("{base}ROLE_A = [\"ROLE_B\"\n")).contains("invalid TOML"));
        assert!(
            refused(&format!("{base}ROLE_A = []\nROLE_A = [\"ROLE_USER\"]\n"))
                .contains("line 3: invalid TOML")
        );
    }

    #[test]
    fn shared_checks_apply_with_line_numbers() {
        let base = "[security.role_hierarchy]\n";

        assert!(
            refused(&format!("{base}ROLE_A = [\"ROLE_A\"]\n"))
                .contains("line 2: `ROLE_A` cannot inherit itself")
        );
        assert!(
            refused(&format!("{base}ROLE_A = [\"ROLE_B\", \"ROLE_B\"]\n")).contains("listed twice")
        );
        assert!(
            refused(&format!(
                "{base}ROLE_USER = [\"ROLE_ADMIN\"]\nROLE_ADMIN = [\"ROLE_USER\"]\n"
            ))
            .contains("ROLE_USER → ROLE_ADMIN → ROLE_USER")
        );
    }
}
