use axum::{
    body::Body,
    extract::Request,
    response::{IntoResponse, Response},
};
use hyper_util::{client::legacy::Client, rt::TokioExecutor};

/// Reverse proxy handler: forwards all requests to the target URL.
///
/// Used to proxy frontend SSR requests through the Rust backend,
/// so only one port needs to be exposed in production.
#[derive(Clone)]
pub struct FrontendProxy {
    target: String,
    client: Client<hyper_util::client::legacy::connect::HttpConnector, Body>,
}

impl FrontendProxy {
    pub fn new(target: &str) -> Self {
        let client = Client::builder(TokioExecutor::new())
            .build_http();

        Self {
            target: target.trim_end_matches('/').to_string(),
            client,
        }
    }

    /// Axum handler that proxies the request to the frontend
    pub async fn handle(self, req: Request) -> Response {
        match self.proxy(req).await {
            Ok(resp) => resp,
            Err(e) => {
                tracing::error!("Proxy error: {e}");
                (
                    axum::http::StatusCode::BAD_GATEWAY,
                    "Bad gateway".to_string(),
                )
                    .into_response()
            }
        }
    }

    async fn proxy(&self, req: Request) -> Result<Response, Box<dyn std::error::Error + Send + Sync>> {
        let path = req.uri().path_and_query()
            .map(|pq| pq.as_str())
            .unwrap_or("/");

        let uri = format!("{}{}", self.target, path).parse::<hyper::Uri>()?;

        // Rebuild the request with the new URI
        let (mut parts, body) = req.into_parts();
        parts.uri = uri;

        let proxy_req = Request::from_parts(parts, body);
        let resp = self.client.request(proxy_req).await?;

        Ok(resp.into_response())
    }
}
