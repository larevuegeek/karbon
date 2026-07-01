//! Live debug mode — the Karbon equivalent of Symfony's `app_dev.php`.
//!
//! On a compiled, single-process Rust binary there is no second "dev front controller"
//! to hit. Instead this module enables a **per-request** debug mode on a live deployment:
//! append `?__karbon_dev=<KEY>` to any URL and — if the request comes from an allowed IP —
//! a signed, IP-bound cookie is set. While that cookie is valid, *your* browsing runs in
//! debug mode (verbose errors, debug toolbar, Studio access) while every other visitor
//! stays in production mode.
//!
//! Security model:
//! - Activation requires the secret `KARBON_DEBUG_KEY` **and** the real client IP to be in
//!   `KARBON_DEBUG_IPS`. The real IP is resolved through `TRUSTED_PROXIES` (X-Forwarded-For),
//!   never the loopback peer of a same-host reverse proxy.
//! - The cookie payload embeds the client IP and an expiry, signed with HMAC-SHA256. A stolen
//!   cookie replayed from another network fails the IP check; a tampered cookie fails the MAC.
//! - Disabled entirely unless `KARBON_DEBUG_KEY` is set (or a local debug build).

use std::net::IpAddr;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::body::Body;
use axum::extract::Request;
use axum::http::{HeaderValue, StatusCode, Uri, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::security::Crypto;
use crate::util::HttpHelper;

const ACTIVATION_PARAM: &str = "__karbon_dev";
const COOKIE_NAME: &str = "_karbon_dev";
const SESSION_TTL_SECS: i64 = 3600; // 1 hour

tokio::task_local! {
    static DEV_MODE: bool;
}

/// Whether the current request runs in live debug mode.
///
/// Reads a task-local set by [`run`]. Returns `false` when the middleware is not installed
/// (plain production), so callers fall back to their own env-based defaults.
pub fn active() -> bool {
    DEV_MODE.try_with(|v| *v).unwrap_or(false)
}

/// Runs `fut` inside a debug-mode scope. Test helper — lets code exercise the
/// `active()`-gated branches without going through the full middleware.
#[cfg(test)]
pub(crate) async fn scoped<F, T>(active: bool, fut: F) -> T
where
    F: std::future::Future<Output = T>,
{
    DEV_MODE.scope(active, fut).await
}

/// Everything the middleware needs, captured once at wiring time.
#[derive(Clone)]
pub struct DevModeConfig {
    /// Secret required to activate a debug session. `None` disables activation.
    pub key: Option<String>,
    /// Client IPs allowed to activate / hold a debug session.
    pub allowed_ips: Vec<String>,
    /// Trusted reverse proxies, for resolving the real client IP.
    pub trusted_proxies: Vec<String>,
    /// HMAC key for signing the session cookie.
    pub signing_key: Vec<u8>,
    /// Local debug builds (`karbon dev`): the site is dev for everyone.
    pub always_on: bool,
}

/// Middleware body. Wire it via a `from_fn` closure that supplies `cfg` and the peer address.
pub async fn run(peer: IpAddr, cfg: DevModeConfig, request: Request, next: Next) -> Response {
    let real_ip = HttpHelper::client_ip_trusted(request.headers(), peer, &cfg.trusted_proxies);

    // Activation / deactivation via query param → set/clear cookie, strip the param, redirect.
    if let Some(action) = activation_action(request.uri().query()) {
        return handle_activation(action, &cfg, real_ip, &request);
    }

    let session = cfg.always_on || cookie_valid(&request, &cfg, real_ip);
    DEV_MODE.scope(session, next.run(request)).await
}

enum Action {
    Activate(String),
    Deactivate,
}

fn activation_action(query: Option<&str>) -> Option<Action> {
    let q = query?;
    let raw = q.split('&').find_map(|kv| {
        kv.strip_prefix(ACTIVATION_PARAM)
            .and_then(|r| r.strip_prefix('='))
    })?;
    if raw == "off" {
        Some(Action::Deactivate)
    } else {
        Some(Action::Activate(raw.to_string()))
    }
}

fn handle_activation(
    action: Action,
    cfg: &DevModeConfig,
    real_ip: IpAddr,
    request: &Request,
) -> Response {
    let location = strip_param(request.uri());
    let https = is_https(request);
    match action {
        Action::Deactivate => redirect(&location, Some(clear_cookie(https))),
        Action::Activate(key) => {
            let key_ok = cfg
                .key
                .as_deref()
                .is_some_and(|k| !k.is_empty() && constant_time_eq(&key, k));
            if key_ok && ip_allowed(real_ip, &cfg.allowed_ips) {
                redirect(&location, Some(session_cookie(real_ip, cfg, https)))
            } else {
                // Fail closed: strip the secret from the URL, set nothing.
                redirect(&location, None)
            }
        }
    }
}

fn cookie_valid(request: &Request, cfg: &DevModeConfig, real_ip: IpAddr) -> bool {
    let Some(raw) = cookie_value(request, COOKIE_NAME) else {
        return false;
    };
    // value = "<exp>|<ip>|<sig>"
    let Some((payload, sig)) = raw.rsplit_once('|') else {
        return false;
    };
    if !constant_time_eq(sig, &sign(payload, &cfg.signing_key)) {
        return false;
    }
    let Some((exp_s, ip_s)) = payload.split_once('|') else {
        return false;
    };
    let Ok(exp) = exp_s.parse::<i64>() else {
        return false;
    };
    if now_unix() > exp {
        return false;
    }
    if ip_s != real_ip.to_string() {
        return false;
    }
    ip_allowed(real_ip, &cfg.allowed_ips)
}

fn session_cookie(ip: IpAddr, cfg: &DevModeConfig, https: bool) -> HeaderValue {
    let exp = now_unix() + SESSION_TTL_SECS;
    let payload = format!("{exp}|{ip}");
    let value = format!("{payload}|{}", sign(&payload, &cfg.signing_key));
    let secure = if https { "; Secure" } else { "" };
    let cookie = format!(
        "{COOKIE_NAME}={value}; Path=/; HttpOnly; SameSite=Strict; Max-Age={SESSION_TTL_SECS}{secure}"
    );
    HeaderValue::from_str(&cookie).unwrap_or_else(|_| HeaderValue::from_static(""))
}

fn clear_cookie(https: bool) -> HeaderValue {
    let secure = if https { "; Secure" } else { "" };
    HeaderValue::from_str(&format!(
        "{COOKIE_NAME}=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0{secure}"
    ))
    .unwrap_or_else(|_| HeaderValue::from_static(""))
}

fn redirect(location: &str, set_cookie: Option<HeaderValue>) -> Response {
    let location =
        HeaderValue::from_str(location).unwrap_or_else(|_| HeaderValue::from_static("/"));
    let mut resp = match Response::builder()
        .status(StatusCode::SEE_OTHER)
        .header(header::LOCATION, location)
        .body(Body::empty())
    {
        Ok(r) => r,
        Err(_) => return StatusCode::SEE_OTHER.into_response(),
    };
    if let Some(cookie) = set_cookie {
        resp.headers_mut().append(header::SET_COOKIE, cookie);
    }
    resp
}

fn sign(payload: &str, key: &[u8]) -> String {
    Crypto::hash_token_keyed(payload, key)
}

fn ip_allowed(ip: IpAddr, allowed: &[String]) -> bool {
    let s = ip.to_string();
    allowed.iter().any(|a| a == &s)
}

fn strip_param(uri: &Uri) -> String {
    let path = uri.path();
    let kept: Vec<&str> = uri
        .query()
        .map(|q| {
            q.split('&')
                .filter(|kv| {
                    !kv.strip_prefix(ACTIVATION_PARAM)
                        .is_some_and(|r| r.starts_with('='))
                })
                .collect()
        })
        .unwrap_or_default();
    if kept.is_empty() {
        path.to_string()
    } else {
        format!("{path}?{}", kept.join("&"))
    }
}

fn is_https(request: &Request) -> bool {
    request.uri().scheme_str() == Some("https")
        || request
            .headers()
            .get("x-forwarded-proto")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.eq_ignore_ascii_case("https"))
}

fn cookie_value(request: &Request, name: &str) -> Option<String> {
    let cookies = request.headers().get(header::COOKIE)?.to_str().ok()?;
    for part in cookies.split(';') {
        let part = part.trim();
        if let Some(v) = part.strip_prefix(name).and_then(|r| r.strip_prefix('=')) {
            return Some(v.to_string());
        }
    }
    None
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn constant_time_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> DevModeConfig {
        DevModeConfig {
            key: Some("s3cret-key".into()),
            allowed_ips: vec!["1.2.3.4".into()],
            trusted_proxies: Vec::new(),
            signing_key: b"signing-secret".to_vec(),
            always_on: false,
        }
    }

    fn req_with_cookie(cookie: &str) -> Request {
        Request::builder()
            .uri("/")
            .header(header::COOKIE, cookie)
            .body(Body::empty())
            .unwrap()
    }

    #[test]
    fn valid_cookie_from_allowed_ip_is_accepted() {
        let c = cfg();
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        let payload = format!("{}|{}", now_unix() + 60, ip);
        let value = format!("{payload}|{}", sign(&payload, &c.signing_key));
        assert!(cookie_valid(
            &req_with_cookie(&format!("{COOKIE_NAME}={value}")),
            &c,
            ip
        ));
    }

    #[test]
    fn cookie_bound_to_a_different_ip_is_rejected() {
        let c = cfg();
        let signed_ip: IpAddr = "1.2.3.4".parse().unwrap();
        let payload = format!("{}|{}", now_unix() + 60, signed_ip);
        let value = format!("{payload}|{}", sign(&payload, &c.signing_key));
        // Same cookie, replayed from another (even if allowed-list) IP → rejected.
        let other: IpAddr = "9.9.9.9".parse().unwrap();
        assert!(!cookie_valid(
            &req_with_cookie(&format!("{COOKIE_NAME}={value}")),
            &c,
            other
        ));
    }

    #[test]
    fn tampered_signature_is_rejected() {
        let c = cfg();
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        let payload = format!("{}|{}", now_unix() + 60, ip);
        let value = format!("{payload}|deadbeef");
        assert!(!cookie_valid(
            &req_with_cookie(&format!("{COOKIE_NAME}={value}")),
            &c,
            ip
        ));
    }

    #[test]
    fn expired_cookie_is_rejected() {
        let c = cfg();
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        let payload = format!("{}|{}", now_unix() - 1, ip);
        let value = format!("{payload}|{}", sign(&payload, &c.signing_key));
        assert!(!cookie_valid(
            &req_with_cookie(&format!("{COOKIE_NAME}={value}")),
            &c,
            ip
        ));
    }

    #[test]
    fn ip_not_in_allowlist_is_rejected_even_with_valid_signature() {
        let mut c = cfg();
        c.allowed_ips = vec!["5.5.5.5".into()];
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        let payload = format!("{}|{}", now_unix() + 60, ip);
        let value = format!("{payload}|{}", sign(&payload, &c.signing_key));
        assert!(!cookie_valid(
            &req_with_cookie(&format!("{COOKIE_NAME}={value}")),
            &c,
            ip
        ));
    }

    #[test]
    fn strip_param_removes_only_the_activation_key() {
        let uri: Uri = "/blog?a=1&__karbon_dev=x&b=2".parse().unwrap();
        assert_eq!(strip_param(&uri), "/blog?a=1&b=2");
        let uri2: Uri = "/blog?__karbon_dev=x".parse().unwrap();
        assert_eq!(strip_param(&uri2), "/blog");
    }

    #[tokio::test]
    async fn active_reflects_scope() {
        assert!(!active());
        assert!(scoped(true, async { active() }).await);
        assert!(!scoped(false, async { active() }).await);
    }
}
