use std::env;
use std::fs;

/// Application environment (Symfony-style).
///
/// Resolved from the `APP_ENV` variable. Unknown values are treated as custom
/// environments and kept as-is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Environment {
    Development,
    Production,
    Test,
    Custom(String),
}

impl Environment {
    pub fn from_name(name: &str) -> Self {
        match name {
            "dev" | "development" => Self::Development,
            "prod" | "production" => Self::Production,
            "test" | "testing" => Self::Test,
            other => Self::Custom(other.to_string()),
        }
    }

    /// Canonical name used to resolve `.env.{name}` files.
    pub fn name(&self) -> &str {
        match self {
            Self::Development => "development",
            Self::Production => "production",
            Self::Test => "test",
            Self::Custom(s) => s,
        }
    }

    pub fn is_development(&self) -> bool {
        matches!(self, Self::Development)
    }

    pub fn is_production(&self) -> bool {
        matches!(self, Self::Production)
    }

    pub fn is_test(&self) -> bool {
        matches!(self, Self::Test)
    }
}

/// Load environment variables from the `.env` family of files, in cascade.
///
/// Precedence (highest first — a value already set is never overwritten,
/// matching Symfony's behavior):
///
/// 1. Real OS environment variables
/// 2. `.env.{env}.local`
/// 3. `.env.local`        (skipped in the `test` environment)
/// 4. `.env.{env}`
/// 5. `.env`
///
/// The resolved environment is also written back to `APP_ENV` so the rest of
/// the application can rely on it being present.
///
/// Returns the resolved [`Environment`].
pub fn load_env() -> Environment {
    let env_name = resolve_env_name();
    let env = Environment::from_name(&env_name);
    let name = env.name();

    // Highest precedence first; dotenvy keeps the first value it sees.
    let _ = dotenvy::from_filename(format!(".env.{name}.local"));
    if !env.is_test() {
        let _ = dotenvy::from_filename(".env.local");
    }
    let _ = dotenvy::from_filename(format!(".env.{name}"));
    let _ = dotenvy::from_filename(".env");

    if env::var("APP_ENV").is_err() {
        unsafe { env::set_var("APP_ENV", name) };
    }

    env
}

/// Resolve the environment name without polluting the process env yet.
/// OS variable wins, then `.env.local`, then `.env`, else `development`.
fn resolve_env_name() -> String {
    if let Ok(v) = env::var("APP_ENV")
        && !v.trim().is_empty()
    {
        return v;
    }
    read_dotenv_key(".env.local", "APP_ENV")
        .or_else(|| read_dotenv_key(".env", "APP_ENV"))
        .unwrap_or_else(|| "development".to_string())
}

/// Minimal `.env` reader for a single key (handles `KEY=value` and optional quotes).
fn read_dotenv_key(file: &str, key: &str) -> Option<String> {
    let content = fs::read_to_string(file).ok()?;
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        if let Some((k, v)) = line.split_once('=')
            && k.trim() == key
        {
            let v = v.trim().trim_matches(|c| c == '"' || c == '\'');
            if !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    None
}
