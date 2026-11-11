//! `karbon user` — accounts of the `users` table that `karbon generate auth` creates,
//! managed from the terminal.
//!
//! Signing up through the application always gives `ROLE_USER`. The first administrator
//! therefore comes from here: `karbon user create you@example.com --admin`.

use argon2::Argon2;
use argon2::password_hash::PasswordHasher;
use colored::Colorize;
use sqlx::AssertSqlSafe;
use sqlx::any::{AnyPoolOptions, install_default_drivers};
use std::io::IsTerminal;
use std::path::Path;

const TABLE: &str = "users";
const USER_ROLES: &str = "ROLE_USER";
const ADMIN_ROLES: &str = "ROLE_USER,ROLE_ADMIN";

pub fn run(root: &Path, action: &str, email: &str, admin: bool, password: Option<&str>) -> Result<(), String> {
    println!("\n{}  user {}\n", "▲ karbon".bold().red(), action.dimmed());

    let email = email.trim().to_lowercase();
    check_email(&email)?;

    match action {
        "create" => {
            let password = match password {
                Some(password) => password.to_string(),
                None => prompt_password()?,
            };
            check_password(&password)?;
            let hash = hash_password(&password)?;
            let roles = if admin { ADMIN_ROLES } else { USER_ROLES };
            block_on(create(root, &email, &hash, roles))?;
            println!("  {} {} created ({})\n", "✓".green().bold(), email.cyan(), roles.dimmed());
            Ok(())
        }
        "promote" => {
            block_on(promote(root, &email))?;
            println!("  {} {} is now an administrator ({})\n", "✓".green().bold(), email.cyan(), ADMIN_ROLES.dimmed());
            println!("    {}\n", "They get the new role at their next sign-in.".dimmed());
            Ok(())
        }
        other => Err(format!("Unknown user action '{other}'. Available: create, promote")),
    }
}

fn block_on<F: std::future::Future<Output = Result<(), String>>>(future: F) -> Result<(), String> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("Failed to start async runtime: {e}"))?
        .block_on(future)
}

async fn connect(root: &Path) -> Result<(sqlx::AnyPool, bool), String> {
    let env_path = root.join(".env");
    let content = std::fs::read_to_string(&env_path)
        .map_err(|_| format!("No {} — copy `.env.example` to `.env` first", env_path.display()))?;
    let vars = content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| line.strip_prefix("export ").unwrap_or(line).split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.trim().trim_matches('"').trim_matches('\'').to_string()))
        .collect();
    let url = crate::db_url::from_vars(&vars, crate::db_url::compiled_driver(root))
        .ok_or("No database configured: set DB_NAME (or DATABASE_URL) in .env")?;

    // A relative SQLite path means the same file here as for the application.
    let _ = std::env::set_current_dir(root);
    install_default_drivers();
    let pool = AnyPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .map_err(|e| format!("Cannot connect to database: {e}"))?;
    Ok((pool, url.starts_with("postgres")))
}

/// `?` for MySQL and SQLite, `$n` for PostgreSQL.
fn placeholder(postgres: bool, n: usize) -> String {
    if postgres { format!("${n}") } else { "?".to_string() }
}

async fn exists(pool: &sqlx::AnyPool, postgres: bool, email: &str) -> Result<bool, String> {
    let sql = format!("SELECT COUNT(*) FROM {TABLE} WHERE email = {}", placeholder(postgres, 1));
    let count: i64 = sqlx::query_scalar(AssertSqlSafe(sql))
        .bind(email)
        .fetch_one(pool)
        .await
        .map_err(|e| {
            format!(
                "Cannot read the `{TABLE}` table: {e}\n       Run `karbon generate auth`, `karbon migrate diff` and \
             `karbon migrate` first."
            )
        })?;
    Ok(count > 0)
}

async fn create(root: &Path, email: &str, hash: &str, roles: &str) -> Result<(), String> {
    let (pool, postgres) = connect(root).await?;
    if exists(&pool, postgres, email).await? {
        return Err(format!(
            "An account already exists with {email}. To make it an administrator: karbon user promote {email}"
        ));
    }
    let p = |n| placeholder(postgres, n);
    let sql = format!("INSERT INTO {TABLE} (email, password, roles) VALUES ({}, {}, {})", p(1), p(2), p(3));
    sqlx::query(AssertSqlSafe(sql))
        .bind(email)
        .bind(hash)
        .bind(roles)
        .execute(&pool)
        .await
        .map_err(|e| format!("Cannot create the account: {e}"))?;
    Ok(())
}

async fn promote(root: &Path, email: &str) -> Result<(), String> {
    let (pool, postgres) = connect(root).await?;
    if !exists(&pool, postgres, email).await? {
        return Err(format!("No account with {email}. To create it: karbon user create {email} --admin"));
    }
    let p = |n| placeholder(postgres, n);
    let sql = format!("UPDATE {TABLE} SET roles = {} WHERE email = {}", p(1), p(2));
    sqlx::query(AssertSqlSafe(sql))
        .bind(ADMIN_ROLES)
        .bind(email)
        .execute(&pool)
        .await
        .map_err(|e| format!("Cannot update the account: {e}"))?;
    Ok(())
}

/// Argon2id with the framework's parameters: `karbon::security::Password::verify` reads
/// the algorithm and its cost from the hash itself.
fn hash_password(password: &str) -> Result<String, String> {
    Argon2::default()
        .hash_password(password.as_bytes())
        .map(|hash| hash.to_string())
        .map_err(|e| format!("Cannot hash the password: {e}"))
}

/// On a terminal the password is read without being echoed, and asked twice: a typo nobody
/// could see must not become the password. Piped input (`echo … | karbon user create …`)
/// is read as a single line.
fn prompt_password() -> Result<String, String> {
    let unreadable = |e: std::io::Error| format!("Cannot read the password: {e}");

    if !std::io::stdin().is_terminal() {
        let mut line = String::new();
        std::io::stdin().read_line(&mut line).map_err(unreadable)?;
        return Ok(line.trim_end_matches(['\r', '\n']).to_string());
    }

    let password = rpassword::prompt_password("  Password: ").map_err(unreadable)?;
    let repeated = rpassword::prompt_password("  Repeat password: ").map_err(unreadable)?;
    if password == repeated {
        Ok(password)
    } else {
        Err("The two passwords differ".to_string())
    }
}

/// Loose on purpose: the point is to catch a typo, not to judge addresses.
fn check_email(email: &str) -> Result<(), String> {
    let valid = email.split_once('@').is_some_and(|(local, domain)| {
        !local.is_empty() && domain.contains('.') && !domain.starts_with('.') && !domain.ends_with('.')
    }) && !email.chars().any(char::is_whitespace);
    if valid { Ok(()) } else { Err(format!("`{email}` is not an email address")) }
}

/// The rule the generated sign-up form applies (the framework's default `Password`
/// constraint): an account created here must be able to change its password there.
fn check_password(password: &str) -> Result<(), String> {
    let strong = password.chars().count() >= 8
        && password.chars().any(|c| c.is_ascii_lowercase())
        && password.chars().any(|c| c.is_ascii_uppercase())
        && password.chars().any(|c| c.is_ascii_digit());
    if strong {
        Ok(())
    } else {
        Err("The password needs at least 8 characters, with a lowercase letter, an uppercase letter and a digit"
            .to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emails_are_checked_loosely() {
        for ok in ["a@b.co", "first.last+tag@sub.example.org"] {
            assert!(check_email(ok).is_ok(), "{ok}");
        }
        for bad in ["", "plain", "@b.co", "a@", "a@b", "a@.co", "a@b.", "a b@c.de"] {
            assert!(check_email(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn the_password_rule_is_the_sign_up_form_s() {
        assert!(check_password("Str0ngPass").is_ok());
        for weak in ["Sh0rt", "alllowercase1", "ALLUPPERCASE1", "NoDigitsHere", ""] {
            assert!(check_password(weak).is_err(), "{weak:?}");
        }
    }

    #[test]
    fn hashes_are_argon2id_and_salted() {
        let (a, b) = (hash_password("Str0ngPass").unwrap(), hash_password("Str0ngPass").unwrap());
        assert!(a.starts_with("$argon2id$"), "{a}");
        assert_ne!(a, b, "two hashes of the same password must differ (salt)");
    }

    #[test]
    fn placeholders_follow_the_driver() {
        assert_eq!(placeholder(false, 2), "?");
        assert_eq!(placeholder(true, 2), "$2");
    }
}
