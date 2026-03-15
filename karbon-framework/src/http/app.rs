use axum::{middleware, Router};
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tower_http::compression::CompressionLayer;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

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
}

/// Main application builder
pub struct App {
    config: Config,
    router: Option<Router<AppState>>,
}

impl App {
    /// Create a new App from environment config
    pub fn new() -> Self {
        dotenvy::dotenv().ok();
        let config = Config::from_env();
        Self {
            config,
            router: None,
        }
    }

    /// Create a new App with a specific config
    pub fn with_config(config: Config) -> Self {
        Self {
            config,
            router: None,
        }
    }

    /// Set the application router
    pub fn router(mut self, router: Router<AppState>) -> Self {
        self.router = Some(router);
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
        let cors = CorsLayer::new()
            .allow_methods(Any)
            .allow_headers(Any);

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
    pub async fn serve(self) -> anyhow::Result<()> {
        self.init_tracing();

        tracing::info!(
            "Connecting to database at {}:{}",
            self.config.db_host,
            self.config.db_port
        );

        let db = Database::connect(&self.config).await?;
        tracing::info!("Database connected");

        // Initialize mailer if SMTP is configured
        let mailer = if !self.config.smtp_host.is_empty() && !self.config.smtp_user.is_empty() {
            match Mailer::new(&self.config) {
                Ok(m) => {
                    tracing::info!("Mailer initialized ({})", self.config.smtp_host);
                    Some(m)
                }
                Err(e) => {
                    tracing::warn!("Mailer initialization failed: {} — emails will be disabled", e);
                    None
                }
            }
        } else {
            tracing::info!("Mailer not configured — emails disabled");
            None
        };

        let state = AppState {
            db,
            config: self.config.clone(),
            mailer,
            role_hierarchy: crate::security::default_hierarchy(),
        };

        let cors = self.cors_layer();
        let mut router = self
            .router
            .unwrap_or_else(Router::new);

        // Profiler activé uniquement en mode debug
        if cfg!(debug_assertions) {
            tracing::info!("Debug profiler enabled");
            router = router.layer(middleware::from_fn(crate::logger::profiler_middleware));
        }

        // Frontend reverse proxy (enabled via KARBON_FRONTEND_URL env var)
        if let Ok(frontend_url) = std::env::var("KARBON_FRONTEND_URL") {
            tracing::info!("Frontend proxy enabled → {}", frontend_url);
            let proxy = super::FrontendProxy::new(&frontend_url);
            router = router.fallback(move |req| proxy.clone().handle(req));
        }

        let router = router
            .layer(middleware::from_fn(super::middleware::request_id))
            .layer(CompressionLayer::new())
            .layer(cors)
            .layer(TraceLayer::new_for_http())
            .with_state(state);

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
