use std::net::SocketAddr;
use tokio::net::TcpListener;

use crate::config::Config;
use crate::db::{Database, DbPool};
use crate::http::AppState;
use crate::mail::Mailer;


/// Test application helper. Boots the app on a random port for integration tests.
///
/// ```ignore
/// #[tokio::test]
/// async fn test_health() {
///     let app = TestApp::spawn(build_router()).await;
///
///     let res = app.get("/health").await;
///     assert_eq!(res.status(), 200);
///
///     let body: serde_json::Value = res.json().await;
///     assert_eq!(body["status"], "ok");
/// }
/// ```
pub struct TestApp {
    pub addr: SocketAddr,
    pub pool: DbPool,
    pub client: reqwest::Client,
}

impl TestApp {
    /// Spawn the app on a random port. Requires DATABASE_URL or DB_* env vars.
    pub async fn spawn(router: axum::Router<AppState>) -> Self {
        dotenvy::dotenv().ok();
        let config = Config::from_env();
        let db = Database::connect(&config)
            .await
            .expect("Failed to connect to test database");

        let pool = db.pool().clone();

        let mailer = if !config.smtp_host.is_empty() && !config.smtp_user.is_empty() {
            Mailer::new(&config).ok()
        } else {
            None
        };

        let state = AppState {
            db,
            config,
            mailer,
            role_hierarchy: crate::security::default_hierarchy(),
            #[cfg(feature = "templates")]
            templates: crate::template::TemplateEngine::empty(),
        };

        let router = router.with_state(state);

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("Failed to bind test listener");
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            if let Err(e) = axum::serve(
                listener,
                router.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
            {
                tracing::error!("Test server error: {e}");
            }
        });

        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("Failed to build HTTP client");

        Self { addr, pool, client }
    }

    /// Base URL for the test server
    pub fn url(&self, path: &str) -> String {
        format!("http://{}{}", self.addr, path)
    }

    /// GET request
    pub async fn get(&self, path: &str) -> reqwest::Response {
        self.client
            .get(&self.url(path))
            .send()
            .await
            .expect("Failed to send GET request")
    }

    /// POST request with JSON body
    pub async fn post_json<T: serde::Serialize>(&self, path: &str, body: &T) -> reqwest::Response {
        self.client
            .post(&self.url(path))
            .json(body)
            .send()
            .await
            .expect("Failed to send POST request")
    }

    /// PUT request with JSON body
    pub async fn put_json<T: serde::Serialize>(&self, path: &str, body: &T) -> reqwest::Response {
        self.client
            .put(&self.url(path))
            .json(body)
            .send()
            .await
            .expect("Failed to send PUT request")
    }

    /// DELETE request
    pub async fn delete(&self, path: &str) -> reqwest::Response {
        self.client
            .delete(&self.url(path))
            .send()
            .await
            .expect("Failed to send DELETE request")
    }

    /// GET request with auth header
    pub async fn get_auth(&self, path: &str, token: &str) -> reqwest::Response {
        self.client
            .get(&self.url(path))
            .bearer_auth(token)
            .send()
            .await
            .expect("Failed to send authenticated GET request")
    }
}
