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

    // CSRF (double-submit + same-site Origin check). Enabled by default.
    pub csrf_enabled: bool,

    // Trusted reverse proxies (IPs). When the direct peer is one of these, the
    // `X-Forwarded-*` headers it sets are trusted (client IP, proto, host). Empty = trust
    // none (Karbon is the edge). `*` = always behind a trusted proxy.
    pub trusted_proxies: Vec<String>,

    // Live debug mode ("app_dev"): secret key to activate a per-request debug session,
    // plus the IP allowlist allowed to activate/hold it. `debug_key` None ⇒ disabled.
    pub debug_key: Option<String>,
    pub debug_ips: Vec<String>,

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

    // Rate limiting
    pub rate_limit_max: u32,    // max requests per window
    pub rate_limit_window: u64, // window in seconds

    // Body size
    pub body_max_size: usize, // max request body in bytes

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
            #[cfg(feature = "sqlite")]
            db_port: 0,
            // DB_NAME empty → the app runs without a database (see `has_database`).
            db_name: env_or("DB_NAME", ""),
            db_user: env_or("DB_USER", ""),
            db_password: env_or("DB_PASSWORD", ""),
            db_max_connections: env_parse("DB_MAX_CONNECTIONS", 10),

            // JWT — empty means auth is unavailable (tokens can't be verified).
            jwt_secret: env_or("JWT_SECRET", ""),
            jwt_expiration: env_parse("JWT_EXPIRATION", 900), // 15min
            refresh_token_expiration: env_parse("REFRESH_TOKEN_EXPIRATION", 2_592_000), // 30 days

            // CORS — default deny (empty ⇒ restrictive same-origin fallback). Set
            // CORS_ORIGINS explicitly to allow cross-origin clients; "*" allows any.
            cors_origins: env_or("CORS_ORIGINS", "")
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),

            // CSRF protection (opt-out with CSRF_ENABLED=false)
            csrf_enabled: env_parse("CSRF_ENABLED", true),

            // Trusted reverse proxies (comma list of IPs, or "*"). Empty = trust none.
            trusted_proxies: env_or("TRUSTED_PROXIES", "")
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),

            // Live debug mode (Symfony-style app_dev). Disabled unless KARBON_DEBUG_KEY is set.
            debug_key: {
                let k = env_or("KARBON_DEBUG_KEY", "");
                if k.is_empty() { None } else { Some(k) }
            },
            debug_ips: env_or("KARBON_DEBUG_IPS", "")
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
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

            // Rate limiting (0 = disabled)
            rate_limit_max: env_parse("RATE_LIMIT_MAX", 0),
            rate_limit_window: env_parse("RATE_LIMIT_WINDOW", 60),

            // Body size (default 10MB)
            body_max_size: env_parse("BODY_MAX_SIZE", 10_485_760),

            // Site
            site_name: env_or("SITE_NAME", "Karbon"),
            site_url: env_or("SITE_URL", "http://localhost:3000"),
            base_url: env_or("BASE_URL", "http://localhost:3000/"),
        }
    }

    /// Create a minimal config for unit tests (no env vars needed)
    pub fn test_config(jwt_secret: &str) -> Self {
        Self {
            port: 3005,
            environment: "test".into(),
            log_level: "error".into(),
            db_host: "127.0.0.1".into(),
            db_port: 3306,
            db_name: "test".into(),
            db_user: "test".into(),
            db_password: "test".into(),
            db_max_connections: 1,
            jwt_secret: jwt_secret.into(),
            jwt_expiration: 3600,
            refresh_token_expiration: 86400,
            cors_origins: vec!["*".into()],
            csrf_enabled: true,
            trusted_proxies: Vec::new(),
            debug_key: None,
            debug_ips: Vec::new(),
            upload_dir: "/tmp".into(),
            upload_max_size: 10_485_760,
            cdn_url: "".into(),
            mail_from: "test@test.com".into(),
            smtp_host: "localhost".into(),
            smtp_port: 587,
            smtp_user: "".into(),
            smtp_password: "".into(),
            rate_limit_max: 0,
            rate_limit_window: 60,
            body_max_size: 10_485_760,
            site_name: "Test".into(),
            site_url: "http://localhost:3005".into(),
            base_url: "http://localhost:3005/".into(),
        }
    }

    /// Resolved application environment.
    pub fn environment(&self) -> super::Environment {
        super::Environment::from_name(&self.environment)
    }

    /// Check if running in production
    pub fn is_production(&self) -> bool {
        self.environment().is_production()
    }

    /// Check if running in development
    pub fn is_development(&self) -> bool {
        self.environment().is_development()
    }

    /// Whether a database is configured (i.e. `DB_NAME` is set). When false, the
    /// app runs without connecting to a database.
    pub fn has_database(&self) -> bool {
        !self.db_name.is_empty()
    }

    /// Check if running in the test environment
    pub fn is_test(&self) -> bool {
        self.environment().is_test()
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

    /// Get full database URL.
    ///
    /// For SQLite, `DB_NAME` is the database **file path** (e.g. `app.db` or
    /// `:memory:`). `?mode=rwc` creates the file if it does not exist.
    #[cfg(feature = "sqlite")]
    pub fn database_url(&self) -> String {
        if self.db_name.is_empty() || self.db_name == ":memory:" {
            "sqlite::memory:".to_string()
        } else {
            format!("sqlite://{}?mode=rwc", self.db_name)
        }
    }
}

/// Get an env var or return a default value
fn env_or(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_string())
}

/// Parse an env var into a type, with a default
fn env_parse<T: std::str::FromStr>(key: &str, default: T) -> T {
    env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}
