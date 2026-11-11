mod builder;
mod collector;
mod handlers;
pub mod middleware;

pub use builder::{build, build_with_token};

/// The Studio auth token, injected into request extensions so the debug toolbar
/// can build an authenticated `/_studio?token=…` link.
#[derive(Clone)]
pub struct StudioToken(pub String);

/// Whether a request reached this process over loopback **and** names it as such.
///
/// The peer address alone is not enough to waive the Studio token:
/// - after DNS rebinding, a page from another site reaches `127.0.0.1` from the developer's
///   own browser — but its `Host` is still the attacker's name;
/// - any page the developer visits may open a WebSocket or send a form to `localhost` — but
///   it says where it comes from in `Origin`.
///
/// So the `Host` must be a loopback name, and so must the `Origin` when there is one (the
/// frontend dev server on another local port is fine, another site is not).
pub(crate) fn addressed_locally(peer: std::net::IpAddr, headers: &axum::http::HeaderMap) -> bool {
    let header = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());

    if !peer.is_loopback() || !header("host").is_some_and(is_loopback_authority) {
        return false;
    }
    match header("origin") {
        Some(origin) => origin
            .split_once("://")
            .is_some_and(|(_, authority)| is_loopback_authority(authority)),
        None => true,
    }
}

/// `localhost`, `127.0.0.1` or `[::1]`, with or without a port.
fn is_loopback_authority(authority: &str) -> bool {
    let host = match authority.strip_prefix('[') {
        Some(rest) => match rest.split_once(']') {
            Some((ip, after)) if after.is_empty() || after.starts_with(':') => ip,
            _ => return false,
        },
        None => authority.split(':').next().unwrap_or(authority),
    };
    host.eq_ignore_ascii_case("localhost") || host.parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

/// Karbon framework version (used by the toolbar + Studio info panel).
pub(crate) const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Features compiled into this build (shown in the toolbar + Studio).
pub(crate) fn enabled_features() -> Vec<&'static str> {
    let mut f = Vec::new();
    if cfg!(feature = "mysql") {
        f.push("mysql");
    }
    if cfg!(feature = "postgres") {
        f.push("postgres");
    }
    if cfg!(feature = "sqlite") {
        f.push("sqlite");
    }
    if cfg!(feature = "templates") {
        f.push("templates");
    }
    if cfg!(feature = "minijinja") {
        f.push("minijinja");
    }
    if cfg!(feature = "native-templates") {
        f.push("native-templates");
    }
    if cfg!(feature = "scraping") {
        f.push("scraping");
    }
    if cfg!(feature = "redis") {
        f.push("redis");
    }
    if cfg!(feature = "studio") {
        f.push("studio");
    }
    if cfg!(feature = "cgi") {
        f.push("cgi");
    }
    f
}
pub use collector::{
    EventRecord, JobRecord, JobStatus, MailRecord, MailStatus, RequestRecord, StudioCollector, StudioMessage,
    StudioStats,
};

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;
    use std::net::IpAddr;

    const LOOPBACK: &str = "127.0.0.1";

    fn local(peer: &str, pairs: &[(&'static str, &str)]) -> bool {
        let mut headers = HeaderMap::new();
        for (name, value) in pairs {
            headers.insert(*name, value.parse().unwrap());
        }
        addressed_locally(peer.parse::<IpAddr>().unwrap(), &headers)
    }

    #[test]
    fn a_browser_on_the_same_machine_is_local() {
        assert!(local(LOOPBACK, &[("host", "localhost:3005")]));
        assert!(local(LOOPBACK, &[("host", "127.0.0.1:3005")]));
        assert!(local("::1", &[("host", "[::1]:3005")]));
        assert!(local(LOOPBACK, &[("host", "LOCALHOST")]));
    }

    #[test]
    fn a_network_peer_is_never_local() {
        assert!(!local("192.168.1.20", &[("host", "localhost:3005")]));
        assert!(!local("192.168.1.20", &[("host", "192.168.1.5:3005")]));
    }

    /// DNS rebinding: the attacker's page talks to 127.0.0.1, under its own name.
    #[test]
    fn a_rebound_hostname_is_not_local() {
        assert!(!local(LOOPBACK, &[("host", "evil.test:3005")]));
        assert!(!local(LOOPBACK, &[("host", "localhost.evil.test:3005")]));
        assert!(!local(LOOPBACK, &[("host", "127.0.0.1.evil.test")]));
        assert!(!local(LOOPBACK, &[("host", "[::1]evil.test")]));
        assert!(!local(LOOPBACK, &[]));
    }

    /// Cross-site WebSocket or form: right peer, right Host, foreign Origin.
    #[test]
    fn a_foreign_origin_is_not_local() {
        let host = ("host", "localhost:3005");
        assert!(!local(LOOPBACK, &[host, ("origin", "https://evil.test")]));
        assert!(!local(LOOPBACK, &[host, ("origin", "null")]));
        assert!(!local(LOOPBACK, &[host, ("origin", "http://localhost.evil.test")]));
    }

    /// The Vite dev server proxies `/_studio` from another local port.
    #[test]
    fn the_frontend_dev_server_origin_is_local() {
        let host = ("host", "localhost:3004");
        assert!(local(LOOPBACK, &[host, ("origin", "http://localhost:3004")]));
        assert!(local(LOOPBACK, &[host, ("origin", "http://127.0.0.1:5173")]));
        assert!(local(LOOPBACK, &[host, ("origin", "http://[::1]:3004")]));
    }
}
