use std::net::SocketAddr;
use std::sync::Arc;

use axum::{
    extract::{ConnectInfo, Request},
    http::{HeaderValue, Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::security::Crypto;
use crate::util::HttpHelper;

const CSRF_HEADER: &str = "x-csrf-token";
const CSRF_COOKIE: &str = "csrf_token";

/// What the CSRF middleware needs to judge an incoming `Origin`.
///
/// Built from [`crate::config::Config`] by [`crate::http::App`]. [`CsrfConfig::default`]
/// — no allowed origins, no trusted proxies — reproduces the historical behaviour:
/// strict same-origin, compared against the raw `Host` header.
#[derive(Clone, Debug, Default)]
pub struct CsrfConfig {
    /// Origins allowed to send unsafe cross-origin requests, from `CORS_ORIGINS`.
    pub allowed_origins: Arc<Vec<String>>,
    /// Reverse proxies whose `X-Forwarded-Host` may be believed, from `TRUSTED_PROXIES`.
    pub trusted_proxies: Arc<Vec<String>>,
}

impl CsrfConfig {
    pub fn new(allowed_origins: Vec<String>, trusted_proxies: Vec<String>) -> Self {
        Self {
            allowed_origins: Arc::new(allowed_origins),
            trusted_proxies: Arc::new(trusted_proxies),
        }
    }
}

/// CSRF protection middleware, using the application's own configuration.
///
/// - On safe methods (GET, HEAD, OPTIONS): sets a CSRF cookie if not already present.
/// - On unsafe methods (POST, PUT, PATCH, DELETE): rejects an `Origin` the deployment
///   never declared, then validates that the `X-CSRF-Token` header matches the
///   `csrf_token` cookie.
///
/// API endpoints using only Bearer token auth (no cookies) can skip this
/// by not sending cookies at all — the middleware only enforces when a
/// CSRF cookie is present.
pub async fn csrf_protection_with(config: CsrfConfig, request: Request, next: Next) -> Response {
    let method = request.method().clone();
    let is_safe = matches!(method, Method::GET | Method::HEAD | Method::OPTIONS);

    if is_safe {
        // Check HTTPS before consuming request — honor a TLS-terminating reverse proxy
        // that forwards over plain HTTP with X-Forwarded-Proto: https.
        let is_https = request.uri().scheme_str() == Some("https")
            || request
                .headers()
                .get("x-forwarded-proto")
                .and_then(|v| v.to_str().ok())
                .is_some_and(|v| v.eq_ignore_ascii_case("https"));
        let mut response = next.run(request).await;
        // Set CSRF cookie if not already present
        if !has_csrf_cookie(&response) {
            let token = Crypto::random_token(32);
            // HttpOnly intentionally NOT set: JS must read the token to send it in X-CSRF-Token header.
            // SameSite=Strict prevents cross-site request forgery.
            let secure = if is_https { "; Secure" } else { "" };
            let cookie = format!(
                "{}={}; Path=/; SameSite=Strict; Max-Age=86400{}",
                CSRF_COOKIE, token, secure
            );
            if let Ok(val) = HeaderValue::from_str(&cookie) {
                response.headers_mut().append("set-cookie", val);
            }
        }
        return response;
    }

    // Unsafe method. Pure Bearer-token API clients (no cookies) are exempt — cross-site
    // attackers cannot set an Authorization header from a browser.
    if has_bearer_auth(&request) {
        return next.run(request).await;
    }

    // Defense-in-depth: reject an Origin/Referer the deployment never declared. Browsers
    // always send Origin on unsafe cross-origin requests, so this blocks classic CSRF even
    // before the first CSRF cookie has been issued (e.g. a bare login POST).
    if let Some(false) = origin_is_accepted(&request, &config) {
        return (StatusCode::FORBIDDEN, "Cross-site request rejected").into_response();
    }

    // Double-submit: if a CSRF cookie is present, the X-CSRF-Token header must match it.
    let cookie_token = extract_csrf_cookie(&request);
    let header_token = request
        .headers()
        .get(CSRF_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    match cookie_token {
        Some(cookie_val) => match header_token {
            Some(header_val) if constant_time_eq(&header_val, &cookie_val) => {
                next.run(request).await
            }
            _ => (StatusCode::FORBIDDEN, "CSRF token mismatch").into_response(),
        },
        // No cookie yet and an accepted origin (or no Origin header): allow. The Origin
        // check above already rejected cross-site attempts.
        None => next.run(request).await,
    }
}

/// CSRF protection with no allowlist and no trusted proxy: strict same-origin against the
/// raw `Host` header. Prefer wiring [`csrf_protection_with`] with the application's
/// [`CsrfConfig`] — [`crate::http::App`] does — as this variant rejects every unsafe
/// request when the router sits behind a proxy that rewrites `Host`.
pub async fn csrf_protection(request: Request, next: Next) -> Response {
    csrf_protection_with(CsrfConfig::default(), request, next).await
}

fn has_bearer_auth(request: &Request) -> bool {
    request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.to_ascii_lowercase().starts_with("bearer "))
}

/// Returns `Some(true)` if the request's `Origin`/`Referer` may send an unsafe method,
/// `Some(false)` if it is cross-site, and `None` if no Origin/Referer is present.
///
/// An origin is accepted when `CORS_ORIGINS` explicitly allows it, or when it matches the
/// host the browser actually addressed (see [`public_host`]).
fn origin_is_accepted(request: &Request, config: &CsrfConfig) -> Option<bool> {
    let source = request
        .headers()
        .get("origin")
        .or_else(|| request.headers().get("referer"))
        .and_then(|v| v.to_str().ok())?;

    // Naming an origin in CORS_ORIGINS states that it may talk to this application;
    // refusing every unsafe method from it would make that setting a lie. The
    // double-submit token below still applies — this only relaxes the Origin check.
    if config
        .allowed_origins
        .iter()
        .any(|entry| allowlist_covers(entry, source))
    {
        return Some(true);
    }

    let host = public_host(request, config)?;
    let (_, source_host) = split_origin(source);
    Some(source_host.eq_ignore_ascii_case(&host))
}

/// The host the browser actually addressed.
///
/// `Host` alone is not it: behind a reverse proxy that header carries whatever the proxy
/// chose to send upstream. nginx's default is the *upstream* name, and Karbon's own proxy
/// ([`crate::http::proxy`]) deliberately rewrites it to prevent Host poisoning — in both
/// cases the public host travels in `X-Forwarded-Host` instead, and comparing `Origin`
/// against a rewritten `Host` rejects every legitimate unsafe request.
///
/// `X-Forwarded-Host` is believed only when the direct peer is a configured trusted proxy;
/// otherwise any client could forge it and walk straight past the same-origin check.
fn public_host(request: &Request, config: &CsrfConfig) -> Option<String> {
    let peer = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ConnectInfo(addr)| addr.ip());

    if let Some(peer) = peer
        && HttpHelper::is_trusted_proxy(peer, &config.trusted_proxies)
        && let Some(forwarded) = HttpHelper::get_header(request.headers(), "x-forwarded-host")
    {
        // `X-Forwarded-Host: public, hop1` — the client-facing host comes first.
        let first = forwarded.split(',').next().unwrap_or(&forwarded).trim();
        if !first.is_empty() {
            return Some(first.to_string());
        }
    }

    HttpHelper::get_header(request.headers(), "host")
}

/// Whether one `CORS_ORIGINS` entry covers this `Origin`/`Referer`.
///
/// `*` deliberately does NOT match. It is a CORS wildcard — browsers refuse to send
/// credentials to it anyway — not a declaration that every site on the web may POST with
/// the user's cookies. Such a deployment keeps the same-origin rule and the token check.
fn allowlist_covers(entry: &str, source: &str) -> bool {
    let entry = entry.trim();
    if entry.is_empty() || entry == "*" {
        return false;
    }

    let (entry_scheme, entry_host) = split_origin(entry);
    let (source_scheme, source_host) = split_origin(source);

    if entry_host.is_empty() || !entry_host.eq_ignore_ascii_case(source_host) {
        return false;
    }

    match (entry_scheme, source_scheme) {
        (Some(entry_scheme), Some(source_scheme)) => {
            entry_scheme.eq_ignore_ascii_case(source_scheme)
        }
        // An entry written without a scheme ("example.com") matches either scheme.
        (None, _) => true,
        // A schemeless Origin is malformed; don't let it satisfy a scheme-qualified entry.
        (Some(_), None) => false,
    }
}

/// Split an `Origin`/`Referer` value into `(scheme, host[:port])`.
/// `https://example.com:8443/a?b` → `(Some("https"), "example.com:8443")`.
fn split_origin(value: &str) -> (Option<&str>, &str) {
    let (scheme, rest) = match value.split_once("://") {
        Some((scheme, rest)) => (Some(scheme), rest),
        None => (None, value),
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    (scheme, host)
}

fn extract_csrf_cookie(request: &Request) -> Option<String> {
    let cookies = request.headers().get("cookie")?.to_str().ok()?;
    for part in cookies.split(';') {
        let part = part.trim();
        if let Some(value) = part.strip_prefix("csrf_token=") {
            return Some(value.to_string());
        }
    }
    None
}

fn has_csrf_cookie(response: &Response) -> bool {
    response
        .headers()
        .get_all("set-cookie")
        .iter()
        .any(|v| v.to_str().is_ok_and(|s| s.starts_with("csrf_token=")))
}

/// Constant-time comparison to prevent timing attacks
fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.bytes()
        .zip(b.bytes())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;

    fn config(origins: &[&str], proxies: &[&str]) -> CsrfConfig {
        CsrfConfig::new(
            origins.iter().map(|s| s.to_string()).collect(),
            proxies.iter().map(|s| s.to_string()).collect(),
        )
    }

    /// Build a POST carrying the given headers, optionally from a known socket peer.
    fn request(headers: &[(&str, &str)], peer: Option<&str>) -> Request {
        let mut builder = Request::builder().method(Method::POST).uri("/api/v1/login");
        for (name, value) in headers {
            builder = builder.header(*name, *value);
        }
        let mut req = builder.body(Body::empty()).unwrap();
        if let Some(peer) = peer {
            let addr: SocketAddr = peer.parse().unwrap();
            req.extensions_mut().insert(ConnectInfo(addr));
        }
        req
    }

    // ─── Same-origin, no proxy (Karbon is the edge) ───

    #[test]
    fn same_origin_is_accepted() {
        let req = request(
            &[("host", "example.com"), ("origin", "https://example.com")],
            None,
        );
        assert_eq!(origin_is_accepted(&req, &CsrfConfig::default()), Some(true));
    }

    #[test]
    fn cross_origin_is_rejected() {
        let req = request(
            &[("host", "example.com"), ("origin", "https://evil.test")],
            None,
        );
        assert_eq!(
            origin_is_accepted(&req, &CsrfConfig::default()),
            Some(false)
        );
    }

    #[test]
    fn a_differing_port_is_still_cross_site() {
        let req = request(
            &[
                ("host", "localhost:3005"),
                ("origin", "http://localhost:3004"),
            ],
            None,
        );
        assert_eq!(
            origin_is_accepted(&req, &CsrfConfig::default()),
            Some(false)
        );
    }

    #[test]
    fn no_origin_header_is_undecided() {
        let req = request(&[("host", "example.com")], None);
        assert_eq!(origin_is_accepted(&req, &CsrfConfig::default()), None);
    }

    #[test]
    fn referer_stands_in_for_a_missing_origin() {
        let req = request(
            &[
                ("host", "example.com"),
                ("referer", "https://example.com/login?next=/"),
            ],
            None,
        );
        assert_eq!(origin_is_accepted(&req, &CsrfConfig::default()), Some(true));
    }

    // ─── CORS_ORIGINS allowlist ───

    #[test]
    fn a_declared_origin_may_post_cross_site() {
        // The dev setup: SvelteKit on :3004 proxying to the backend on :3005.
        let req = request(
            &[
                ("host", "localhost:3005"),
                ("origin", "http://localhost:3004"),
            ],
            None,
        );
        let config = config(&["http://localhost:3004", "http://localhost:3005"], &[]);
        assert_eq!(origin_is_accepted(&req, &config), Some(true));
    }

    #[test]
    fn an_undeclared_origin_is_still_rejected() {
        let req = request(
            &[("host", "localhost:3005"), ("origin", "http://evil.test")],
            None,
        );
        let config = config(&["http://localhost:3004"], &[]);
        assert_eq!(origin_is_accepted(&req, &config), Some(false));
    }

    #[test]
    fn the_allowlist_is_scheme_sensitive() {
        let req = request(
            &[
                ("host", "example.com"),
                ("origin", "http://app.example.com"),
            ],
            None,
        );
        let config = config(&["https://app.example.com"], &[]);
        assert_eq!(origin_is_accepted(&req, &config), Some(false));
    }

    #[test]
    fn a_wildcard_grants_nothing_to_csrf() {
        let req = request(
            &[("host", "example.com"), ("origin", "https://evil.test")],
            None,
        );
        let config = config(&["*"], &[]);
        assert_eq!(origin_is_accepted(&req, &config), Some(false));
    }

    // ─── X-Forwarded-Host behind a trusted proxy ───

    #[test]
    fn a_trusted_proxy_supplies_the_public_host() {
        // nginx default: `Host` becomes the upstream, the public host moves to XFH.
        let req = request(
            &[
                ("host", "127.0.0.1:3005"),
                ("x-forwarded-host", "codissio.com"),
                ("origin", "https://codissio.com"),
            ],
            Some("10.0.0.1:51000"),
        );
        let config = config(&[], &["10.0.0.1"]);
        assert_eq!(origin_is_accepted(&req, &config), Some(true));
    }

    #[test]
    fn an_untrusted_peer_cannot_forge_the_public_host() {
        let req = request(
            &[
                ("host", "codissio.com"),
                ("x-forwarded-host", "evil.test"),
                ("origin", "https://evil.test"),
            ],
            Some("203.0.113.9:51000"),
        );
        let config = config(&[], &["10.0.0.1"]);
        assert_eq!(origin_is_accepted(&req, &config), Some(false));
    }

    #[test]
    fn the_first_forwarded_host_is_the_client_facing_one() {
        let req = request(
            &[
                ("host", "127.0.0.1:3005"),
                ("x-forwarded-host", "codissio.com, internal.lan"),
                ("origin", "https://codissio.com"),
            ],
            Some("10.0.0.1:51000"),
        );
        let config = config(&[], &["10.0.0.1"]);
        assert_eq!(origin_is_accepted(&req, &config), Some(true));
    }

    #[test]
    fn a_wildcard_trusted_proxy_is_honored() {
        let req = request(
            &[
                ("host", "127.0.0.1:3005"),
                ("x-forwarded-host", "codissio.com"),
                ("origin", "https://codissio.com"),
            ],
            Some("10.0.0.1:51000"),
        );
        let config = config(&[], &["*"]);
        assert_eq!(origin_is_accepted(&req, &config), Some(true));
    }

    // ─── Parsing ───

    #[test]
    fn split_origin_keeps_the_port_and_drops_the_path() {
        assert_eq!(
            split_origin("https://example.com:8443/a?b"),
            (Some("https"), "example.com:8443")
        );
        assert_eq!(split_origin("example.com"), (None, "example.com"));
        assert_eq!(split_origin("null"), (None, "null"));
    }

    #[test]
    fn a_schemeless_allowlist_entry_matches_either_scheme() {
        assert!(allowlist_covers("example.com", "https://example.com"));
        assert!(allowlist_covers("example.com", "http://example.com"));
        assert!(!allowlist_covers("example.com", "https://other.test"));
    }
}
