//! Route path-parameter validation rules.
//!
//! Attached to a route via the controller macro:
//! `#[karbon::get("/{id}", id = "int:min=1")]`. The macro injects a call to
//! [`validate`] at the start of the handler, so an invalid path parameter is
//! rejected with **HTTP 400** before the handler logic runs.
//!
//! Rule grammar:
//! - `int`, `int:min=N`, `int:max=N`
//! - `string`, `string:minlen=N`, `string:maxlen=N`
//! - `slug` — lowercase letters/digits/hyphens (no leading/trailing/double `-`)
//! - `uuid` — canonical 8-4-4-4-12 hex form
//! - `enum:a|b|c` — value must be exactly one of the listed alternatives
//! - `regex:<pattern>` — value must match the (anchored as written) regular expression

use crate::error::{AppError, AppResult};
use std::collections::HashMap;
use std::fmt::Display;
use std::sync::{Mutex, OnceLock};

/// Validate the path parameter `value` (named `name`) against `rule`.
/// Returns `AppError::Validation` (400) on failure.
pub fn validate<T: Display>(name: &str, value: &T, rule: &str) -> AppResult<()> {
    let s = value.to_string();

    // `regex:` and `enum:` keep their payload verbatim (it may contain `:` / `|`).
    if let Some(pattern) = rule.strip_prefix("regex:") {
        return check_regex(name, &s, pattern);
    }
    if let Some(list) = rule.strip_prefix("enum:") {
        let allowed: Vec<&str> = list.split('|').filter(|v| !v.is_empty()).collect();
        return if allowed.iter().any(|v| *v == s) {
            Ok(())
        } else {
            Err(err(name, &format!("doit être : {}", allowed.join(", "))))
        };
    }

    let mut parts = rule.split(':');
    match parts.next().unwrap_or("") {
        "int" => {
            let n: i64 = s.parse().map_err(|_| err(name, "doit être un entier"))?;
            for c in parts {
                if let Some(v) = c.strip_prefix("min=")
                    && let Ok(m) = v.parse::<i64>()
                    && n < m
                {
                    return Err(err(name, &format!("doit être ≥ {m}")));
                }
                if let Some(v) = c.strip_prefix("max=")
                    && let Ok(m) = v.parse::<i64>()
                    && n > m
                {
                    return Err(err(name, &format!("doit être ≤ {m}")));
                }
            }
        }
        "string" => {
            let len = s.chars().count();
            for c in parts {
                if let Some(v) = c.strip_prefix("minlen=")
                    && let Ok(m) = v.parse::<usize>()
                    && len < m
                {
                    return Err(err(name, &format!("au moins {m} caractères")));
                }
                if let Some(v) = c.strip_prefix("maxlen=")
                    && let Ok(m) = v.parse::<usize>()
                    && len > m
                {
                    return Err(err(name, &format!("au plus {m} caractères")));
                }
            }
        }
        "slug" if !is_slug(&s) => {
            return Err(err(name, "slug invalide (minuscules, chiffres, tirets)"));
        }
        "uuid" if !is_uuid(&s) => return Err(err(name, "UUID invalide")),
        _ => {}
    }
    Ok(())
}

fn err(name: &str, msg: &str) -> AppError {
    AppError::Validation(format!("{name}: {msg}"))
}

/// Match `s` against a regex `pattern`, caching the compiled regex (and compile
/// failures) keyed by pattern to avoid recompiling on every request.
fn check_regex(name: &str, s: &str, pattern: &str) -> AppResult<()> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<regex::Regex>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = cache.lock().unwrap_or_else(|e| e.into_inner());
    let entry = guard
        .entry(pattern.to_string())
        .or_insert_with(|| regex::Regex::new(pattern).ok());
    match entry {
        Some(re) if re.is_match(s) => Ok(()),
        Some(_) => Err(err(name, "format invalide")),
        None => Err(err(name, "règle regex invalide")), // developer typo in the pattern
    }
}

fn is_slug(s: &str) -> bool {
    !s.is_empty()
        && !s.starts_with('-')
        && !s.ends_with('-')
        && !s.contains("--")
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn is_uuid(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 36
        && b.iter().enumerate().all(|(i, &c)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                c == b'-'
            } else {
                c.is_ascii_hexdigit()
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn int_min_max() {
        assert!(validate("id", &5i64, "int:min=1").is_ok());
        assert!(validate("id", &0i64, "int:min=1").is_err());
        assert!(validate("id", &200i64, "int:max=100").is_err());
        assert!(validate("id", &"abc", "int").is_err());
    }

    #[test]
    fn string_len() {
        assert!(validate("name", &"hi", "string:minlen=2").is_ok());
        assert!(validate("name", &"h", "string:minlen=2").is_err());
        assert!(validate("name", &"toolong", "string:maxlen=3").is_err());
    }

    #[test]
    fn slug_and_uuid() {
        assert!(validate("slug", &"hello-world", "slug").is_ok());
        assert!(validate("slug", &"Hello", "slug").is_err());
        assert!(validate("slug", &"-x", "slug").is_err());
        assert!(validate("id", &"550e8400-e29b-41d4-a716-446655440000", "uuid").is_ok());
        assert!(validate("id", &"nope", "uuid").is_err());
    }

    #[test]
    fn enum_values() {
        assert!(validate("status", &"draft", "enum:draft|published").is_ok());
        assert!(validate("status", &"published", "enum:draft|published").is_ok());
        assert!(validate("status", &"nope", "enum:draft|published").is_err());
    }

    #[test]
    fn regex_pattern() {
        // A pattern containing ':' must survive (HH:MM).
        assert!(validate("t", &"09:30", r"regex:^\d{2}:\d{2}$").is_ok());
        assert!(validate("t", &"9:30", r"regex:^\d{2}:\d{2}$").is_err());
        assert!(validate("code", &"AB12", "regex:^[A-Z]{2}[0-9]{2}$").is_ok());
        // Invalid pattern → rejected (developer error), not a panic.
        assert!(validate("x", &"y", "regex:[").is_err());
    }
}
