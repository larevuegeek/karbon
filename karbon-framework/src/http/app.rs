#[cfg(feature = "studio")]
use axum::Extension;
use axum::{Router, middleware};
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::TcpListener;
use tower_http::compression::CompressionLayer;
use tower_http::cors::{Any, CorsLayer};
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

use crate::config::Config;
use crate::db::Database;
use crate::mail::Mailer;
use crate::security::RoleHierarchy;

/// Application state shared across all handlers
#[derive(Clone)]
pub struct AppState {
    pub db: Database,
    pub config: Config,
    pub mailer: Option<Mailer>,
    pub role_hierarchy: RoleHierarchy,
    #[cfg(feature = "templates")]
    pub templates: crate::template::TemplateEngine,
}

/// A modular unit of an application — the building block of the Karbon kernel.
///
/// A `Module` contributes a set of routes (and, in the future, services and
/// startup hooks). It is the extension seam that the upcoming **bundle** system
/// (third-party plugins, see the roadmap) will build on, and it is already
/// useful today to split an application into self-contained pieces:
///
/// ```ignore
/// struct BlogModule;
/// impl karbon::http::Module for BlogModule {
///     fn name(&self) -> &str { "blog" }
///     fn prefix(&self) -> Option<&'static str> { Some("/blog") }
///     fn routes(&self) -> axum::Router<karbon::http::AppState> {
///         axum::Router::new().route("/", axum::routing::get(|| async { "blog" }))
///     }
/// }
///
/// App::new().module(BlogModule).serve().await?;
/// ```
pub trait Module: Send + Sync + 'static {
    /// Human-readable module name (used in startup logs).
    fn name(&self) -> &str;

    /// Routes contributed by this module.
    fn routes(&self) -> Router<AppState>;

    /// Optional mount prefix. `Some("/admin")` nests the routes under it;
    /// `None` (default) merges them at the application root.
    fn prefix(&self) -> Option<&'static str> {
        None
    }
}

/// A **bundle**: a packaged, reusable extension (third-party plugin) that
/// contributes routes and runs startup logic. Richer than a [`Module`]: it has
/// a `boot` lifecycle hook invoked once the [`AppState`] is built.
///
/// Bundles are **compile-time** plugins (a crate implementing this trait added
/// to `Cargo.toml`) — Rust has no stable ABI for dynamic loading. This is the
/// model the whole Rust ecosystem uses.
///
/// ```ignore
/// struct AuthBundle;
/// impl karbon::http::Bundle for AuthBundle {
///     fn name(&self) -> &str { "auth" }
///     fn prefix(&self) -> Option<&'static str> { Some("/auth") }
///     fn routes(&self) -> Router<AppState> { /* … */ Router::new() }
///     fn boot<'a>(&'a self, state: &'a AppState)
///         -> Pin<Box<dyn Future<Output = AppResult<()>> + Send + 'a>>
///     { Box::pin(async move { /* run migrations, seed, warm caches… */ Ok(()) }) }
/// }
///
/// App::new().bundle(AuthBundle).serve().await?;
/// ```
pub trait Bundle: Send + Sync + 'static {
    /// Human-readable bundle name (used in startup logs).
    fn name(&self) -> &str;

    /// Routes contributed by this bundle.
    fn routes(&self) -> Router<AppState> {
        Router::new()
    }

    /// Optional mount prefix (`Some("/admin")` nests; `None` merges at root).
    fn prefix(&self) -> Option<&'static str> {
        None
    }

    /// Startup hook, run once after the [`AppState`] is built (DB connected).
    fn boot<'a>(
        &'a self,
        _state: &'a AppState,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = crate::error::AppResult<()>> + Send + 'a>>
    {
        Box::pin(async { Ok(()) })
    }
}

/// Main application builder — the Karbon **kernel**: it assembles the
/// configuration, environment, registered [`Module`]s / [`Bundle`]s and the
/// router, then boots the server.
pub struct App {
    config: Config,
    router: Option<Router<AppState>>,
    modules: Vec<Box<dyn Module>>,
    bundles: Vec<Box<dyn Bundle>>,
    access_control: Option<crate::security::AccessControl>,
}

impl App {
    /// Create a new App from environment config
    pub fn new() -> Self {
        crate::config::load_env();
        let config = Config::from_env();
        Self {
            config,
            router: None,
            modules: Vec::new(),
            bundles: Vec::new(),
            access_control: None,
        }
    }

    /// Create a new App with a specific config
    pub fn with_config(config: Config) -> Self {
        Self {
            config,
            router: None,
            modules: Vec::new(),
            bundles: Vec::new(),
            access_control: None,
        }
    }

    /// Set the application router
    pub fn router(mut self, router: Router<AppState>) -> Self {
        self.router = Some(router);
        self
    }

    /// Register a [`Module`]. Its routes are mounted (nested under
    /// [`Module::prefix`] if set, otherwise merged at the root). Modules are
    /// applied in registration order, after the base router set via [`App::router`].
    pub fn module<M: Module>(mut self, module: M) -> Self {
        self.modules.push(Box::new(module));
        self
    }

    /// Register a [`Bundle`]. Its routes are mounted and its `boot` hook runs at
    /// startup, after the database connects.
    pub fn bundle<B: Bundle>(mut self, bundle: B) -> Self {
        self.bundles.push(Box::new(bundle));
        self
    }

    /// Install a Symfony-style **firewall** ([`AccessControl`](crate::security::AccessControl)):
    /// declarative URL-pattern → roles rules enforced centrally before handlers run.
    ///
    /// ```ignore
    /// use karbon::security::AccessControl;
    /// App::new().router(base)
    ///     .firewall(AccessControl::new()
    ///         .public("^/login")
    ///         .rule("^/admin", ["ROLE_ADMIN"]))
    ///     .serve().await?;
    /// ```
    pub fn firewall(mut self, access_control: impl Into<crate::security::AccessControl>) -> Self {
        self.access_control = Some(access_control.into());
        self
    }

    /// Initialize tracing/logging
    fn init_tracing(&self) {
        let filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new(&self.config.log_level));

        tracing_subscriber::registry()
            .with(filter)
            .with(tracing_subscriber::fmt::layer())
            .init();
    }

    /// Build CORS layer from config
    fn cors_layer(&self) -> CorsLayer {
        let cors = CorsLayer::new().allow_methods(Any).allow_headers(Any);

        if self.config.cors_origins.contains(&"*".to_string()) {
            cors.allow_origin(Any)
        } else if self.config.cors_origins.is_empty() {
            tracing::warn!("CORS_ORIGINS is empty — no cross-origin requests will be allowed");
            cors
        } else {
            let origins: Vec<_> = self
                .config
                .cors_origins
                .iter()
                .filter_map(|o| o.parse().ok())
                .collect();
            cors.allow_origin(origins)
        }
    }

    /// Start the server with graceful shutdown support
    pub async fn serve(mut self) -> anyhow::Result<()> {
        self.init_tracing();

        let db = if self.config.has_database() {
            tracing::info!(
                "Connecting to database at {}:{}",
                self.config.db_host,
                self.config.db_port
            );
            let db = Database::connect(&self.config).await?;
            tracing::info!("Database connected");
            db
        } else {
            tracing::info!("No database configured (DB_NAME empty) — running without a database");
            Database::connect_lazy(&self.config)?
        };

        // Fail closed on a missing/weak JWT secret: in production a weak or placeholder
        // secret lets an attacker forge tokens, so we refuse to start rather than warn.
        if self.config.jwt_secret.is_empty() {
            if self.config.is_production() {
                anyhow::bail!(
                    "JWT_SECRET is not set in production. Set a random secret of at least {} bytes \
                     (e.g. `openssl rand -base64 48`) or auth cannot be secured.",
                    crate::security::MIN_JWT_SECRET_LEN
                );
            }
            tracing::warn!(
                "JWT_SECRET is not set — authentication is disabled (all tokens are rejected)"
            );
        } else if crate::security::is_weak_secret(&self.config.jwt_secret) {
            if self.config.is_production() {
                anyhow::bail!(
                    "JWT_SECRET is too weak (a known placeholder or shorter than {} bytes). \
                     Set a strong random secret before deploying to production.",
                    crate::security::MIN_JWT_SECRET_LEN
                );
            }
            tracing::warn!(
                "JWT_SECRET is weak (short or a placeholder) — acceptable in dev, but MUST be \
                 replaced with a strong random value before production."
            );
        }

        // Initialize mailer if SMTP is configured
        let mailer = if !self.config.smtp_host.is_empty() && !self.config.smtp_user.is_empty() {
            match Mailer::new(&self.config) {
                Ok(m) => {
                    tracing::info!("Mailer initialized ({})", self.config.smtp_host);
                    Some(m)
                }
                Err(e) => {
                    tracing::warn!(
                        "Mailer initialization failed: {} — emails will be disabled",
                        e
                    );
                    None
                }
            }
        } else {
            tracing::info!("Mailer not configured — emails disabled");
            None
        };

        // Template engine (optional feature)
        #[cfg(feature = "templates")]
        let templates = {
            let tpl_dir =
                std::env::var("TEMPLATE_DIR").unwrap_or_else(|_| "./templates".to_string());
            if std::path::Path::new(&tpl_dir).exists() {
                match crate::template::TemplateEngine::new(&tpl_dir) {
                    Ok(engine) => engine,
                    Err(e) => {
                        tracing::warn!("Template engine init failed: {} — using empty engine", e);
                        crate::template::TemplateEngine::empty()
                    }
                }
            } else {
                tracing::debug!("No template directory at '{}', using empty engine", tpl_dir);
                crate::template::TemplateEngine::empty()
            }
        };

        let state = AppState {
            db,
            config: self.config.clone(),
            mailer,
            role_hierarchy: crate::security::default_hierarchy(),
            #[cfg(feature = "templates")]
            templates,
        };

        let cors = self.cors_layer();
        let mut router = self.router.unwrap_or_default();

        // Mount registered modules (the kernel's extension seam).
        for module in &self.modules {
            tracing::info!("Registering module: {}", module.name());
            let routes = module.routes();
            router = match module.prefix() {
                Some(prefix) => router.nest(prefix, routes),
                None => router.merge(routes),
            };
        }

        // Mount registered bundles and run their boot hooks.
        for bundle in &self.bundles {
            tracing::info!("Registering bundle: {}", bundle.name());
            let routes = bundle.routes();
            router = match bundle.prefix() {
                Some(prefix) => router.nest(prefix, routes),
                None => router.merge(routes),
            };
            bundle
                .boot(&state)
                .await
                .map_err(|e| anyhow::anyhow!("Bundle '{}' boot failed: {}", bundle.name(), e))?;
        }

        // Profiler activé uniquement en mode debug
        if cfg!(debug_assertions) {
            tracing::info!("Debug profiler enabled");
            router = router.layer(middleware::from_fn(crate::logger::profiler_middleware));
        }

        // Studio (dev dashboard)
        #[cfg(feature = "studio")]
        let studio_enabled = cfg!(debug_assertions)
            || std::env::var("KARBON_STUDIO").is_ok()
            || self.config.debug_key.is_some();
        #[cfg(feature = "studio")]
        let studio_collector = if studio_enabled {
            let studio_db = if self.config.has_database() {
                Some(state.db.pool().clone())
            } else {
                None
            };
            let (studio_router, collector, token) = crate::studio::build(studio_db);
            // Studio router already has its own state, merge at Router<()> level later
            tracing::info!(
                "\x1b[38;5;141m⚡ Studio\x1b[0m → http://{}:{}/_studio?token={}",
                "localhost",
                self.config.port,
                token
            );
            Some((studio_router, collector, token))
        } else {
            None
        };

        // Frontend reverse proxy (enabled via KARBON_FRONTEND_URL env var)
        if let Ok(frontend_url) = std::env::var("KARBON_FRONTEND_URL") {
            tracing::info!("Frontend proxy enabled → {}", frontend_url);
            let proxy = super::FrontendProxy::new(&frontend_url)
                .trust_proxies(self.config.trusted_proxies.clone());
            router = router.fallback(
                move |axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<
                    std::net::SocketAddr,
                >,
                      req: axum::extract::Request| {
                    proxy.clone().handle(addr.ip(), req)
                },
            );
        }

        // Studio debug toolbar — injected into HTML responses (dev only).
        // Layered here so it runs *inner* to compression: it edits the raw HTML
        // before the CompressionLayer (added below) compresses it.
        #[cfg(feature = "studio")]
        if let Some((_, _, token)) = &studio_collector {
            router = router
                .layer(middleware::from_fn(
                    crate::studio::middleware::studio_toolbar_middleware,
                ))
                .layer(Extension(crate::studio::StudioToken(token.clone())));
        }

        // Body size limit
        let body_limit = RequestBodyLimitLayer::new(self.config.body_max_size);

        // Rate limiting (0 = disabled)
        let rate_limit_max = self.config.rate_limit_max;
        let rate_limit_window = self.config.rate_limit_window;

        let csrf_enabled = self.config.csrf_enabled;

        #[allow(unused_mut)]
        let mut router = router
            .layer(middleware::from_fn(super::middleware::request_id))
            .layer(middleware::from_fn(super::middleware::security_headers))
            .layer(body_limit)
            .layer(CompressionLayer::new())
            .layer(cors)
            .layer(TraceLayer::new_for_http());

        // CSRF protection (double-submit cookie + same-site Origin check). On by default;
        // Bearer-token API requests are exempt automatically.
        if csrf_enabled {
            router = router.layer(middleware::from_fn(super::middleware::csrf_protection));
        }

        // Firewall (Symfony-style access_control): URL-pattern → roles, enforced centrally.
        if let Some(ac) = self.access_control.take() {
            let ac = std::sync::Arc::new(ac);
            router = router.layer(middleware::from_fn_with_state(
                state.clone(),
                move |axum::extract::State(st): axum::extract::State<AppState>,
                      req: axum::extract::Request,
                      next: middleware::Next| {
                    let ac = ac.clone();
                    async move { crate::security::firewall::enforce(ac, st, req, next).await }
                },
            ));
        }

        // Apply rate limiting if configured
        if rate_limit_max > 0 {
            tracing::info!(
                "Rate limiting: {} requests per {}s",
                rate_limit_max,
                rate_limit_window
            );
            router = router.layer(
                super::middleware::RateLimitLayer::new(
                    rate_limit_max,
                    Duration::from_secs(rate_limit_window),
                )
                .trust_proxies(self.config.trusted_proxies.clone()),
            );
        }

        // `mut` is only needed when the studio feature reassigns `router` below.
        #[allow(unused_mut)]
        let mut router = router.with_state(state);

        #[cfg(feature = "studio")]
        if let Some((studio_router, collector, _token)) = studio_collector {
            router = router
                .nest("/_studio", studio_router)
                .layer(middleware::from_fn(
                    crate::studio::middleware::studio_middleware,
                ))
                .layer(Extension(collector));
        }

        // Live debug mode ("app_dev"): outermost layer so its per-request scope wraps every
        // inner middleware (toolbar, studio guard) and the error handler. Present in local
        // debug builds and whenever KARBON_DEBUG_KEY is configured; inert otherwise.
        if cfg!(debug_assertions) || self.config.debug_key.is_some() {
            let dev_cfg = crate::http::dev_mode::DevModeConfig {
                key: self.config.debug_key.clone(),
                allowed_ips: self.config.debug_ips.clone(),
                trusted_proxies: self.config.trusted_proxies.clone(),
                signing_key: format!(
                    "{}{}",
                    self.config.jwt_secret,
                    self.config.debug_key.clone().unwrap_or_default()
                )
                .into_bytes(),
                always_on: cfg!(debug_assertions),
            };
            router = router.layer(middleware::from_fn(
                move |axum::extract::ConnectInfo(peer): axum::extract::ConnectInfo<SocketAddr>,
                      req: axum::extract::Request,
                      next: middleware::Next| {
                    let dev_cfg = dev_cfg.clone();
                    async move { crate::http::dev_mode::run(peer.ip(), dev_cfg, req, next).await }
                },
            ));
        }

        let addr = SocketAddr::from(([0, 0, 0, 0], self.config.port));
        tracing::info!("Server starting on http://{}", addr);

        let listener = TcpListener::bind(addr).await?;
        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(shutdown_signal())
        .await?;

        tracing::info!("Server shut down gracefully");
        Ok(())
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

/// Wait for SIGINT (Ctrl+C) or SIGTERM to gracefully shut down
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => tracing::info!("Received Ctrl+C, shutting down..."),
        _ = terminate => tracing::info!("Received SIGTERM, shutting down..."),
    }
}
