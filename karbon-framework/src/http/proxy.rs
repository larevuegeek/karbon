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
/// Hop-by-hop headers (RFC 7230 §6.1) that must never be forwarded by a proxy.
const HOP_BY_HOP: &[&str] = &[
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
];

#[derive(Clone)]
pub struct FrontendProxy {
    target: String,
    /// Host[:port] of the upstream, used to rewrite the forwarded `Host` header.
    target_host: Option<String>,
    /// Trusted reverse proxy IPs (see `Config::trusted_proxies`).
    trusted_proxies: std::sync::Arc<Vec<String>>,
    client: Client<hyper_util::client::legacy::connect::HttpConnector, Body>,
}

impl FrontendProxy {
    pub fn new(target: &str) -> Self {
        let client = Client::builder(TokioExecutor::new()).build_http();
        let trimmed = target.trim_end_matches('/').to_string();
        // Extract host[:port] from the target URL for the upstream Host header.
        let target_host = trimmed
            .split("://")
            .nth(1)
            .unwrap_or(&trimmed)
            .split('/')
            .next()
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        Self {
            target: trimmed,
            target_host,
            trusted_proxies: std::sync::Arc::new(Vec::new()),
            client,
        }
    }

    /// Configure which direct peers are trusted edge proxies (their inbound
    /// `X-Forwarded-*` are preserved and extended rather than replaced).
    pub fn trust_proxies(mut self, proxies: Vec<String>) -> Self {
        self.trusted_proxies = std::sync::Arc::new(proxies);
        self
    }

    /// Axum handler that proxies the request to the frontend
    pub async fn handle(self, peer: std::net::IpAddr, req: Request) -> Response {
        match self.proxy(peer, req).await {
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

    async fn proxy(
        &self,
        peer: std::net::IpAddr,
        req: Request,
    ) -> Result<Response, Box<dyn std::error::Error + Send + Sync>> {
        let path = req
            .uri()
            .path_and_query()
            .map(|pq| pq.as_str())
            .unwrap_or("/");

        let uri = format!("{}{}", self.target, path).parse::<hyper::Uri>()?;

        // Rebuild the request with the new URI
        let (mut parts, body) = req.into_parts();
        parts.uri = uri;

        // Capture the public-facing request info BEFORE mutating headers, so the SSR
        // upstream can reconstruct absolute/canonical URLs (host, scheme, client IP).
        let hdr = |name: &str| {
            parts
                .headers
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(String::from)
        };
        let orig_host = hdr("host");
        let inbound_xff = hdr("x-forwarded-for");
        let inbound_xfp = hdr("x-forwarded-proto");
        let inbound_xfh = hdr("x-forwarded-host");
        let trusted = crate::util::HttpHelper::is_trusted_proxy(peer, &self.trusted_proxies);

        // Collect the extra hop-by-hop header names announced in the request's own
        // `Connection` header before we start mutating the map.
        let mut extra_hop: Vec<String> = Vec::new();
        if let Some(conn) = parts
            .headers
            .get("connection")
            .and_then(|v| v.to_str().ok())
        {
            for name in conn.split(',') {
                let n = name.trim().to_ascii_lowercase();
                if !n.is_empty() {
                    extra_hop.push(n);
                }
            }
        }

        // Strip hop-by-hop headers (request smuggling / response splitting protection).
        for name in HOP_BY_HOP.iter().map(|s| s.to_string()).chain(extra_hop) {
            parts.headers.remove(name.as_str());
        }
        for name in ["x-forwarded-for", "x-forwarded-host", "x-forwarded-proto"] {
            parts.headers.remove(name);
        }

        // Re-set the standard reverse-proxy headers so the SSR sees the PUBLIC host/proto
        // and real client. When behind a trusted edge proxy, its values are preserved and
        // the client chain extended; otherwise we author them from this request + peer.
        let set = |parts: &mut axum::http::request::Parts, name: &'static str, val: String| {
            if let Ok(v) = val.parse() {
                parts.headers.insert(name, v);
            }
        };
        // X-Forwarded-Host: the public host (incoming Host, or the edge's value if trusted).
        if let Some(host) = (trusted.then_some(inbound_xfh).flatten()).or(orig_host.clone()) {
            set(&mut parts, "x-forwarded-host", host);
        }
        // X-Forwarded-Proto: honor a trusted edge's value, else assume https at the edge
        // (Karbon itself is plain HTTP; TLS is terminated upstream in the common setup).
        let proto = trusted
            .then_some(inbound_xfp)
            .flatten()
            .unwrap_or_else(|| "https".to_string());
        set(&mut parts, "x-forwarded-proto", proto);
        // X-Forwarded-For: extend the trusted chain, else just this peer.
        let xff = match (trusted, inbound_xff) {
            (true, Some(chain)) => format!("{chain}, {peer}"),
            _ => peer.to_string(),
        };
        set(&mut parts, "x-forwarded-for", xff);

        // Rewrite Host to the upstream so the SSR backend can't be Host-poisoned.
        if let Some(host) = &self.target_host
            && let Ok(val) = host.parse()
        {
            parts.headers.insert("host", val);
        }

        let proxy_req = Request::from_parts(parts, body);
        let resp = self.client.request(proxy_req).await?;

        Ok(resp.into_response())
    }
}
