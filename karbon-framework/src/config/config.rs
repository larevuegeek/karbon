use std::env;

/// Application configuration loaded from environment variables
#[derive(Debug, Clone)]
pub struct Config {
    // Server
    pub port: u16,
    pub environment: String,
    pub log_level: String,

    // Database
    pub db_host: String,
    pub db_port: u16,
    pub db_name: String,
    pub db_user: String,
    pub db_password: String,
    pub db_max_connections: u32,

    // JWT
    pub jwt_secret: String,
    pub jwt_expiration: i64,           // access token TTL in seconds
    pub refresh_token_expiration: i64, // refresh token TTL in seconds

    // CORS
    pub cors_origins: Vec<String>,

    // Upload
    pub upload_dir: String,
    pub upload_max_size: u64, // bytes

    // CDN
    pub cdn_url: String,

    // Mail
    pub mail_from: String,
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_user: String,
    pub smtp_password: String,

    // Site
    pub site_name: String,
    pub site_url: String,
    pub base_url: String,
}

impl Config {
    /// Load configuration from environment variables
    pub fn from_env() -> Self {
        Self {
            // Server
            port: env_parse("PORT", 3000),
            environment: env_or("APP_ENV", "development"),
            log_level: env_or("LOG_LEVEL", "info"),

            // Database
            db_host: env_or("DB_HOST", "127.0.0.1"),
            #[cfg(feature = "mysql")]
            db_port: env_parse("DB_PORT", 3306),
            #[cfg(feature = "postgres")]
            db_port: env_parse("DB_PORT", 5432),
            db_name: env_required("DB_NAME"),
            db_user: env_required("DB_USER"),
            db_password: env_or("DB_PASSWORD", ""),
            db_max_connections: env_parse("DB_MAX_CONNECTIONS", 10),

            // JWT
            jwt_secret: env_required("JWT_SECRET"),
            jwt_expiration: env_parse("JWT_EXPIRATION", 900),              // 15min
            refresh_token_expiration: env_parse("REFRESH_TOKEN_EXPIRATION", 2_592_000), // 30 days

            // CORS
            cors_origins: env_or("CORS_ORIGINS", "*")
                .split(',')
                .map(|s| s.trim().to_string())
                .collect(),

            // Upload
            upload_dir: env_or("UPLOAD_DIR", "./uploads"),
            upload_max_size: env_parse("UPLOAD_MAX_SIZE", 10_485_760), // 10MB

            // CDN
            cdn_url: env_or("CDN_URL", ""),

            // Mail
            mail_from: env_or("MAIL_FROM", "noreply@localhost"),
            smtp_host: env_or("SMTP_HOST", "localhost"),
            smtp_port: env_parse("SMTP_PORT", 587),
            smtp_user: env_or("SMTP_USER", ""),
            smtp_password: env_or("SMTP_PASSWORD", ""),

            // Site
            site_name: env_or("SITE_NAME", "Karbon"),
            site_url: env_or("SITE_URL", "http://localhost:3000"),
            base_url: env_or("BASE_URL", "http://localhost:3000/"),
        }
    }

    /// Check if running in production
    pub fn is_production(&self) -> bool {
        self.environment == "production"
    }

    /// Get full database URL
    #[cfg(feature = "mysql")]
    pub fn database_url(&self) -> String {
        format!(
            "mysql://{}:{}@{}:{}/{}",
            self.db_user, self.db_password, self.db_host, self.db_port, self.db_name
        )
    }

    /// Get full database URL
    #[cfg(feature = "postgres")]
    pub fn database_url(&self) -> String {
        format!(
            "postgresql://{}:{}@{}:{}/{}",
            self.db_user, self.db_password, self.db_host, self.db_port, self.db_name
        )
    }
}

/// Get an env var or return a default value
fn env_or(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_string())
}

/// Get a required env var, panic if missing
fn env_required(key: &str) -> String {
    env::var(key).unwrap_or_else(|_| panic!("Environment variable {} is required", key))
}

/// Parse an env var into a type, with a default
fn env_parse<T: std::str::FromStr>(key: &str, default: T) -> T {
    env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}
