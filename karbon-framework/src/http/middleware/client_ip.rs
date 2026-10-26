//! Settles the client address once, before anything reads it.
//!
//! `X-Forwarded-For` / `X-Real-IP` are set by a trusted proxy in production, but anyone
//! can write them. This middleware resolves the real client with
//! [`HttpHelper::client_ip_trusted`] (rightmost address that is not a trusted proxy, or the
//! TCP peer when the peer is not trusted), then removes `X-Forwarded-For` and sets
//! `X-Real-IP` to that address: every later reader sees one value, already decided.
//!
//! ```ignore
//! let trusted = TrustedProxies::from_env();
//! let app = router.layer(axum::middleware::from_fn_with_state(trusted, normalize_client_ip));
//! axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await?;
//! ```
//!
//! Without `ConnectInfo` the peer is unknown: both headers are dropped rather than trusted.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use axum::extract::{ConnectInfo, Request, State};
use axum::http::HeaderValue;
use axum::middleware::Next;
use axum::response::Response;

use crate::util::HttpHelper;

const FORWARDED_FOR: &str = "x-forwarded-for";
const REAL_IP: &str = "x-real-ip";

/// Proxies whose `X-Forwarded-For` is believed: addresses, CIDR ranges, or `*`.
#[derive(Debug, Clone, Default)]
pub struct TrustedProxies(Arc<[String]>);

impl TrustedProxies {
    pub fn new(proxies: Vec<String>) -> Self {
        Self(proxies.into())
    }

    /// `TRUSTED_PROXIES`, comma separated — same reading as `Config::trusted_proxies`.
    /// Empty by default: a binary exposed directly to the Internet trusts nobody.
    pub fn from_env() -> Self {
        let proxies = std::env::var("TRUSTED_PROXIES")
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        Self::new(proxies)
    }

    pub fn as_slice(&self) -> &[String] {
        &self.0
    }
}

pub async fn normalize_client_ip(
    State(trusted): State<TrustedProxies>,
    mut req: Request,
    next: Next,
) -> Response {
    let peer = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ci| ci.0.ip());

    match peer {
        Some(peer) => {
            let client = HttpHelper::client_ip_trusted(req.headers(), peer, trusted.as_slice());
            set_client(&mut req, client);
        }
        None => {
            req.headers_mut().remove(FORWARDED_FOR);
            req.headers_mut().remove(REAL_IP);
        }
    }

    next.run(req).await
}

fn set_client(req: &mut Request, client: IpAddr) {
    let headers = req.headers_mut();

    headers.remove(FORWARDED_FOR);
    headers.remove(REAL_IP);

    if let Ok(value) = HeaderValue::from_str(&client.to_string()) {
        headers.insert(REAL_IP, value);
    }
}

#[cfg(test)]
mod tests {
    use axum::Router;
    use axum::body::Body;
    use axum::routing::get;
    use tower::ServiceExt;

    use super::*;

    async fn seen_ip(trusted: &[&str], peer: &str, forwarded: Option<&str>) -> String {
        let trusted = TrustedProxies::new(trusted.iter().map(|s| s.to_string()).collect());
        let app = Router::new()
            .route(
                "/",
                get(|headers: axum::http::HeaderMap| async move {
                    let forwarded_left = headers.contains_key(FORWARDED_FOR);
                    let real = headers
                        .get(REAL_IP)
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("")
                        .to_string();
                    format!("{real}|{forwarded_left}")
                }),
            )
            .layer(axum::middleware::from_fn_with_state(
                trusted,
                normalize_client_ip,
            ));

        let mut req = axum::http::Request::builder().uri("/");
        if let Some(value) = forwarded {
            req = req.header(FORWARDED_FOR, value).header(REAL_IP, "9.9.9.9");
        }
        let mut req = req.body(Body::empty()).unwrap();
        let peer: SocketAddr = format!("{peer}:4000").parse().unwrap();
        req.extensions_mut().insert(ConnectInfo(peer));

        let response = app.oneshot(req).await.unwrap();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();

        String::from_utf8(bytes.to_vec()).unwrap()
    }

    #[tokio::test]
    async fn behind_a_trusted_proxy_the_appended_address_wins() {
        assert_eq!(
            seen_ip(&["127.0.0.1"], "127.0.0.1", Some("1.2.3.4, 203.0.113.5")).await,
            "203.0.113.5|false"
        );
    }

    #[tokio::test]
    async fn an_untrusted_peer_cannot_choose_its_address() {
        assert_eq!(
            seen_ip(&["127.0.0.1"], "198.51.100.7", Some("1.2.3.4")).await,
            "198.51.100.7|false"
        );
    }

    #[tokio::test]
    async fn without_headers_the_peer_is_the_client() {
        assert_eq!(
            seen_ip(&[], "198.51.100.7", None).await,
            "198.51.100.7|false"
        );
    }
}
