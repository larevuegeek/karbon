use colored::Colorize;
use std::fs;
use std::path::Path;

const KEY: &str = "JWT_SECRET";

/// Secrets shipped in templates and examples — the framework's `PLACEHOLDER_SECRETS`,
/// which the CLI cannot import (it does not depend on the framework). They are public, so
/// the framework refuses to start with any of them.
pub const PLACEHOLDERS: &[&str] =
    &["change-me-to-a-secure-random-string", "change-me-to-a-random-secret", "change-me", "secret", "changeme"];

/// The framework's `MIN_JWT_SECRET_LEN`.
pub const MIN_LEN: usize = 32;

/// 256 random bits as 64 hex characters: nothing a `.env` line would interpret (no quote,
/// no `#`, no space).
pub fn generate() -> String {
    use rand::RngExt;
    let bytes: [u8; 32] = rand::rng().random();
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// `content` with its `JWT_SECRET` line set to `secret`, or with the line appended when
/// the file has none. Comments and every other line are kept as they are.
pub fn set_in_env(content: &str, secret: &str) -> String {
    let eol = if content.contains("\r\n") { "\r\n" } else { "\n" };
    let mut replaced = false;

    let mut lines: Vec<String> = content
        .lines()
        .map(|line| {
            let trimmed = line.trim_start();
            let (export, rest) = match trimmed.strip_prefix("export ") {
                Some(rest) => ("export ", rest),
                None => ("", trimmed),
            };
            match rest.split_once('=') {
                Some((key, _)) if !replaced && key.trim() == KEY => {
                    replaced = true;
                    format!("{export}{KEY}={secret}")
                }
                _ => line.to_string(),
            }
        })
        .collect();

    if !replaced {
        lines.push(format!("{KEY}={secret}"));
    }
    let mut out = lines.join(eol);
    out.push_str(eol);
    out
}

/// `karbon secret` prints a new secret; `karbon secret --write` stores it in `.env`.
pub fn run(root: &Path, write: bool) -> Result<(), String> {
    let secret = generate();

    if !write {
        // The secret alone on stdout, so it can be piped or captured.
        println!("{secret}");
        eprintln!(
            "\n  {} set it as {KEY} in your .env, or run {} to do it.",
            "ℹ".blue(),
            "karbon secret --write".bold()
        );
        return Ok(());
    }

    let env_path = root.join(".env");
    let content = fs::read_to_string(&env_path)
        .map_err(|e| format!("Cannot read {}: {e} — copy `.env.example` to `.env` first", env_path.display()))?;
    fs::write(&env_path, set_in_env(&content, &secret))
        .map_err(|e| format!("Cannot write {}: {e}", env_path.display()))?;

    println!("\n  {} {KEY} written to {}", "✓".green(), env_path.display().to_string().cyan());
    println!("    {}", "Tokens signed with the previous secret are no longer valid: users log in again.".dimmed());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_generated_secret_is_long_random_and_env_safe() {
        let a = generate();
        let b = generate();
        assert_eq!(a.len(), 64);
        assert!(a.len() >= MIN_LEN);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
        assert!(!PLACEHOLDERS.contains(&a.as_str()));
    }

    #[test]
    fn the_secret_line_is_replaced_in_place() {
        let env = "PORT=3005\n# Auth\nJWT_SECRET=change-me-to-a-random-secret\nJWT_EXPIRATION=900\n";
        assert_eq!(set_in_env(env, "abc"), "PORT=3005\n# Auth\nJWT_SECRET=abc\nJWT_EXPIRATION=900\n");
    }

    #[test]
    fn a_commented_line_is_not_the_secret() {
        let env = "# JWT_SECRET=old\nPORT=1\n";
        assert_eq!(set_in_env(env, "abc"), "# JWT_SECRET=old\nPORT=1\nJWT_SECRET=abc\n");
    }

    #[test]
    fn export_prefix_and_crlf_are_kept() {
        let env = "PORT=1\r\nexport JWT_SECRET=\"old\"\r\n";
        assert_eq!(set_in_env(env, "abc"), "PORT=1\r\nexport JWT_SECRET=abc\r\n");
    }

    #[test]
    fn only_the_first_definition_is_rewritten() {
        // dotenv keeps the first value it reads; that is the one to replace.
        let env = "JWT_SECRET=a\nJWT_SECRET=b\n";
        assert_eq!(set_in_env(env, "x"), "JWT_SECRET=x\nJWT_SECRET=b\n");
    }

    #[test]
    fn other_keys_sharing_the_prefix_are_untouched() {
        let env = "JWT_SECRET_OLD=keep\nJWT_SECRET=a\n";
        assert_eq!(set_in_env(env, "x"), "JWT_SECRET_OLD=keep\nJWT_SECRET=x\n");
    }
}
