use axum::http::HeaderMap;
use std::net::{IpAddr, SocketAddr};

/// Parsed User-Agent info
#[derive(Debug, Clone, Default)]
pub struct UserAgentInfo {
    pub browser: Option<String>,
    pub browser_version: Option<String>,
    pub os: Option<String>,
    pub is_mobile: bool,
    pub is_bot: bool,
    pub raw: String,
}

/// HTTP request helpers for extracting client info
pub struct HttpHelper;

impl HttpHelper {
    // ─── User-Agent ───

    /// Parse a User-Agent string into structured info
    pub fn parse_user_agent(ua: &str) -> UserAgentInfo {
        if ua.is_empty() {
            return UserAgentInfo::default();
        }

        let is_bot = Self::detect_bot(ua);
        let is_mobile = ua.contains("Mobile")
            || ua.contains("Android")
            || ua.contains("iPhone")
            || ua.contains("iPad");

        let (browser, browser_version) = Self::detect_browser(ua);
        let os = Self::detect_os(ua);

        UserAgentInfo {
            browser,
            browser_version,
            os,
            is_mobile,
            is_bot,
            raw: ua.to_string(),
        }
    }

    /// Parse User-Agent directly from headers
    pub fn parse_user_agent_from_headers(headers: &HeaderMap) -> UserAgentInfo {
        let ua = Self::get_header(headers, "user-agent").unwrap_or_default();
        Self::parse_user_agent(&ua)
    }

    fn detect_browser(ua: &str) -> (Option<String>, Option<String>) {
        // Order matters: check more specific patterns first
        let patterns: &[(&str, &str, &str)] = &[
            ("OPR/", "Opera", "OPR/"),
            ("Edg/", "Edge", "Edg/"),
            ("Firefox/", "Firefox", "Firefox/"),
            ("Vivaldi/", "Vivaldi", "Vivaldi/"),
            ("Brave", "Brave", "Chrome/"),
            // Chrome must come after Edge, Opera, Vivaldi, Brave
            ("Chrome/", "Chrome", "Chrome/"),
            // Safari must come last (many browsers include Safari in UA)
            ("Safari/", "Safari", "Version/"),
        ];

        for (marker, name, version_prefix) in patterns {
            if ua.contains(marker) {
                let version = Self::extract_version(ua, version_prefix);
                return (Some(name.to_string()), version);
            }
        }

        if !ua.is_empty() {
            (Some("Other".to_string()), None)
        } else {
            (None, None)
        }
    }

    fn detect_os(ua: &str) -> Option<String> {
        // Order matters: Android contains "Linux", iOS check before Mac
        if ua.contains("Android") {
            Some("Android".to_string())
        } else if ua.contains("iPhone") || ua.contains("iPad") || ua.contains("iPod") {
            Some("iOS".to_string())
        } else if ua.contains("Windows NT 10") {
            Some("Windows 10/11".to_string())
        } else if ua.contains("Windows") {
            Some("Windows".to_string())
        } else if ua.contains("Mac OS X") || ua.contains("macOS") {
            Some("macOS".to_string())
        } else if ua.contains("CrOS") {
            Some("Chrome OS".to_string())
        } else if ua.contains("Linux") {
            Some("Linux".to_string())
        } else {
            None
        }
    }

    fn detect_bot(ua: &str) -> bool {
        let bot_markers = [
            "bot",
            "Bot",
            "crawler",
            "Crawler",
            "spider",
            "Spider",
            "Googlebot",
            "Bingbot",
            "Slurp",
            "DuckDuckBot",
            "Baiduspider",
            "YandexBot",
            "facebookexternalhit",
            "Twitterbot",
            "LinkedInBot",
            "WhatsApp",
            "Discordbot",
            "Slackbot",
            "TelegramBot",
            "curl/",
            "wget/",
            "python-requests",
            "Go-http-client",
            "HeadlessChrome",
            "PhantomJS",
            "Lighthouse",
        ];
        bot_markers.iter().any(|m| ua.contains(m))
    }

    /// Extract version number from a UA pattern like "Chrome/120.0.6099.130"
    fn extract_version(ua: &str, prefix: &str) -> Option<String> {
        ua.find(prefix)
            .map(|pos| {
                let start = pos + prefix.len();
                let version: String = ua[start..]
                    .chars()
                    .take_while(|c| c.is_ascii_digit() || *c == '.')
                    .collect();
                version
            })
            .filter(|v| !v.is_empty())
    }

    // ─── IP Address ───

    /// Every address of the `X-Forwarded-For` chain, left (client side) to right (nearest
    /// proxy), across repeated headers. Entries that are not IP addresses are skipped.
    fn forwarded_chain(headers: &HeaderMap) -> Vec<IpAddr> {
        headers
            .get_all("x-forwarded-for")
            .iter()
            .filter_map(|v| v.to_str().ok())
            .flat_map(|v| v.split(','))
            .filter_map(|ip| ip.trim().parse::<IpAddr>().ok())
            .collect()
    }

    fn real_ip_header(headers: &HeaderMap) -> Option<IpAddr> {
        Self::get_header(headers, "x-real-ip").and_then(|ip| ip.trim().parse().ok())
    }

    /// Client IP from `X-Forwarded-For` / `X-Real-Ip`, without knowing which proxies to trust.
    ///
    /// Takes the **last** `X-Forwarded-For` entry, the one appended by the nearest proxy:
    /// earlier entries are written by the client and can be anything. Even so, these headers
    /// are only meaningful behind a reverse proxy: when Karbon is the edge, any visitor can
    /// send them. Prefer [`client_ip_trusted`](Self::client_ip_trusted).
    pub fn client_ip(headers: &HeaderMap, fallback: IpAddr) -> IpAddr {
        Self::forwarded_chain(headers)
            .last()
            .copied()
            .or_else(|| Self::real_ip_header(headers))
            .unwrap_or(fallback)
    }

    /// Extract client IP as string, with SocketAddr fallback (common Axum pattern).
    /// Same caveat as [`client_ip`](Self::client_ip): prefer [`client_ip_trusted`](Self::client_ip_trusted).
    pub fn client_ip_from_socket(headers: &HeaderMap, socket: &SocketAddr) -> String {
        Self::client_ip(headers, socket.ip()).to_string()
    }

    /// Whether `peer` is a configured trusted reverse proxy: an exact address
    /// (`127.0.0.1`, `::1`), a CIDR range (`10.0.0.0/8`, `2001:db8::/32`), or `*` for every peer.
    pub fn is_trusted_proxy(peer: IpAddr, trusted: &[String]) -> bool {
        trusted
            .iter()
            .any(|t| t == "*" || Self::matches_proxy(peer, t))
    }

    /// Exact address or CIDR range (`*` excluded)
    fn matches_proxy(ip: IpAddr, entry: &str) -> bool {
        let entry = entry.trim();
        let Some((network, prefix)) = entry.split_once('/') else {
            return entry.parse::<IpAddr>().is_ok_and(|t| t == ip);
        };
        let (Ok(network), Ok(prefix)) = (network.parse::<IpAddr>(), prefix.parse::<u32>()) else {
            return false;
        };
        match (network, ip) {
            (IpAddr::V4(net), IpAddr::V4(ip)) if prefix <= 32 => {
                let mask = u32::MAX.checked_shl(32 - prefix).unwrap_or(0);
                u32::from(net) & mask == u32::from(ip) & mask
            }
            (IpAddr::V6(net), IpAddr::V6(ip)) if prefix <= 128 => {
                let mask = u128::MAX.checked_shl(128 - prefix).unwrap_or(0);
                u128::from(net) & mask == u128::from(ip) & mask
            }
            _ => false,
        }
    }

    /// Resolve the real client IP with proxy awareness:
    /// - if the direct `peer` is not a trusted proxy, ignore the forwarding headers (they'd
    ///   be client-spoofable) and use `peer`;
    /// - otherwise walk `X-Forwarded-For` **from the right**, skipping the trusted proxies,
    ///   and return the first address that is not one: the client as seen by our own proxy
    ///   chain. Entries further left were written by the client and are never used.
    ///
    /// With `TRUSTED_PROXIES=*`, only the direct peer is trusted: the last entry is used.
    /// Behind a CDN, list its ranges (CIDR) so its edge addresses are skipped too.
    pub fn client_ip_trusted(headers: &HeaderMap, peer: IpAddr, trusted: &[String]) -> IpAddr {
        if !Self::is_trusted_proxy(peer, trusted) {
            return peer;
        }
        let chain = Self::forwarded_chain(headers);
        chain
            .iter()
            .rev()
            .find(|ip| !trusted.iter().any(|t| Self::matches_proxy(**ip, t)))
            // Every hop is a trusted proxy (internal call): the origin of the chain
            .or(chain.first())
            .copied()
            .or_else(|| Self::real_ip_header(headers))
            .unwrap_or(peer)
    }

    // ─── Header Helpers ───

    /// Get a header value as string
    pub fn get_header(headers: &HeaderMap, name: &str) -> Option<String> {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
    }

    /// Get the Referer header
    pub fn referer(headers: &HeaderMap) -> Option<String> {
        Self::get_header(headers, "referer")
    }

    /// Get the Accept-Language header and extract the primary language
    /// e.g., "fr-FR,fr;q=0.9,en-US;q=0.8" → "fr-FR"
    pub fn accept_language(headers: &HeaderMap) -> Option<String> {
        Self::get_header(headers, "accept-language")
            .and_then(|val| val.split(',').next().map(|s| s.trim().to_string()))
    }

    /// Get the Content-Type header
    pub fn content_type(headers: &HeaderMap) -> Option<String> {
        Self::get_header(headers, "content-type")
    }

    /// Check if the request accepts JSON
    pub fn accepts_json(headers: &HeaderMap) -> bool {
        Self::get_header(headers, "accept")
            .map(|v| v.contains("application/json"))
            .unwrap_or(false)
    }

    /// Get the Authorization Bearer token
    pub fn bearer_token(headers: &HeaderMap) -> Option<String> {
        Self::get_header(headers, "authorization")
            .filter(|v| v.starts_with("Bearer "))
            .map(|v| v[7..].to_string())
    }

    /// Check if the request is an AJAX/XHR request
    pub fn is_xhr(headers: &HeaderMap) -> bool {
        Self::get_header(headers, "x-requested-with")
            .map(|v| v.eq_ignore_ascii_case("XMLHttpRequest"))
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── User-Agent parsing ───

    #[test]
    fn test_chrome_windows() {
        let ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.6099.130 Safari/537.36";
        let info = HttpHelper::parse_user_agent(ua);
        assert_eq!(info.browser.as_deref(), Some("Chrome"));
        assert_eq!(info.browser_version.as_deref(), Some("120.0.6099.130"));
        assert_eq!(info.os.as_deref(), Some("Windows 10/11"));
        assert!(!info.is_mobile);
        assert!(!info.is_bot);
    }

    #[test]
    fn test_firefox_linux() {
        let ua = "Mozilla/5.0 (X11; Linux x86_64; rv:121.0) Gecko/20100101 Firefox/121.0";
        let info = HttpHelper::parse_user_agent(ua);
        assert_eq!(info.browser.as_deref(), Some("Firefox"));
        assert_eq!(info.browser_version.as_deref(), Some("121.0"));
        assert_eq!(info.os.as_deref(), Some("Linux"));
    }

    #[test]
    fn test_safari_macos() {
        let ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X 14_2) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.2 Safari/605.1.15";
        let info = HttpHelper::parse_user_agent(ua);
        assert_eq!(info.browser.as_deref(), Some("Safari"));
        assert_eq!(info.browser_version.as_deref(), Some("17.2"));
        assert_eq!(info.os.as_deref(), Some("macOS"));
    }

    #[test]
    fn test_edge() {
        let ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36 Edg/120.0.2210.91";
        let info = HttpHelper::parse_user_agent(ua);
        assert_eq!(info.browser.as_deref(), Some("Edge"));
        assert_eq!(info.browser_version.as_deref(), Some("120.0.2210.91"));
    }

    #[test]
    fn test_mobile_android() {
        let ua = "Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.6099.144 Mobile Safari/537.36";
        let info = HttpHelper::parse_user_agent(ua);
        assert_eq!(info.browser.as_deref(), Some("Chrome"));
        assert_eq!(info.os.as_deref(), Some("Android"));
        assert!(info.is_mobile);
    }

    #[test]
    fn test_iphone_safari() {
        let ua = "Mozilla/5.0 (iPhone; CPU iPhone OS 17_2 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.2 Mobile/15E148 Safari/604.1";
        let info = HttpHelper::parse_user_agent(ua);
        assert_eq!(info.browser.as_deref(), Some("Safari"));
        assert_eq!(info.os.as_deref(), Some("iOS"));
        assert!(info.is_mobile);
    }

    #[test]
    fn test_googlebot() {
        let ua = "Mozilla/5.0 (compatible; Googlebot/2.1; +http://www.google.com/bot.html)";
        let info = HttpHelper::parse_user_agent(ua);
        assert!(info.is_bot);
    }

    #[test]
    fn test_curl() {
        let ua = "curl/8.4.0";
        let info = HttpHelper::parse_user_agent(ua);
        assert!(info.is_bot);
    }

    #[test]
    fn test_empty_ua() {
        let info = HttpHelper::parse_user_agent("");
        assert!(info.browser.is_none());
        assert!(info.os.is_none());
        assert!(!info.is_mobile);
        assert!(!info.is_bot);
    }

    #[test]
    fn test_opera() {
        let ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36 OPR/106.0.0.0";
        let info = HttpHelper::parse_user_agent(ua);
        assert_eq!(info.browser.as_deref(), Some("Opera"));
    }

    // ─── IP extraction ───

    #[test]
    fn test_ip_from_x_forwarded_for() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            "203.0.113.50, 70.41.3.18".parse().unwrap(),
        );
        let fallback: IpAddr = "127.0.0.1".parse().unwrap();
        let ip = HttpHelper::client_ip(&headers, fallback);
        // The last entry is the one our proxy appended; the first one is client-written
        assert_eq!(ip.to_string(), "70.41.3.18");
    }

    fn xff(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", value.parse().unwrap());
        headers
    }

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    fn proxies(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn trusted_ip_ignores_a_spoofed_first_entry() {
        // The visitor 203.0.113.9 sent "X-Forwarded-For: 1.2.3.4"; the local proxy appended it
        let h = xff("1.2.3.4, 203.0.113.9");
        let ip_seen = HttpHelper::client_ip_trusted(&h, ip("127.0.0.1"), &proxies(&["127.0.0.1"]));
        assert_eq!(ip_seen, ip("203.0.113.9"));
    }

    #[test]
    fn trusted_ip_skips_every_trusted_hop() {
        // Apache → Karbon (frontend proxy) → SSR → Karbon: "visitor, 127.0.0.1"
        let h = xff("203.0.113.9, 127.0.0.1");
        let ip_seen =
            HttpHelper::client_ip_trusted(&h, ip("127.0.0.1"), &proxies(&["127.0.0.1", "::1"]));
        assert_eq!(ip_seen, ip("203.0.113.9"));
    }

    #[test]
    fn trusted_ip_honours_cidr_ranges() {
        // CDN edge in 173.245.48.0/20 in front of the local proxy
        let h = xff("1.2.3.4, 198.51.100.7, 173.245.48.10");
        let list = proxies(&["127.0.0.1", "173.245.48.0/20"]);
        assert_eq!(
            HttpHelper::client_ip_trusted(&h, ip("127.0.0.1"), &list),
            ip("198.51.100.7")
        );
        assert!(HttpHelper::is_trusted_proxy(
            ip("2001:db8::1"),
            &proxies(&["2001:db8::/32"])
        ));
        assert!(!HttpHelper::is_trusted_proxy(
            ip("10.0.0.1"),
            &proxies(&["10.0.0.0/33"])
        ));
    }

    #[test]
    fn trusted_ip_with_a_wildcard_uses_the_last_entry() {
        let h = xff("1.2.3.4, 203.0.113.9");
        assert_eq!(
            HttpHelper::client_ip_trusted(&h, ip("10.1.2.3"), &proxies(&["*"])),
            ip("203.0.113.9")
        );
    }

    #[test]
    fn untrusted_peer_ignores_forwarding_headers() {
        let h = xff("1.2.3.4");
        assert_eq!(
            HttpHelper::client_ip_trusted(&h, ip("198.51.100.7"), &proxies(&["127.0.0.1"])),
            ip("198.51.100.7")
        );
    }

    #[test]
    fn trusted_ip_falls_back_to_real_ip_then_peer() {
        let mut h = HeaderMap::new();
        h.insert("x-real-ip", "203.0.113.50".parse().unwrap());
        let list = proxies(&["127.0.0.1"]);
        assert_eq!(
            HttpHelper::client_ip_trusted(&h, ip("127.0.0.1"), &list),
            ip("203.0.113.50")
        );
        assert_eq!(
            HttpHelper::client_ip_trusted(&HeaderMap::new(), ip("127.0.0.1"), &list),
            ip("127.0.0.1")
        );
    }

    #[test]
    fn test_ip_from_x_real_ip() {
        let mut headers = HeaderMap::new();
        headers.insert("x-real-ip", "203.0.113.50".parse().unwrap());
        let fallback: IpAddr = "127.0.0.1".parse().unwrap();
        let ip = HttpHelper::client_ip(&headers, fallback);
        assert_eq!(ip.to_string(), "203.0.113.50");
    }

    #[test]
    fn test_ip_forwarded_takes_priority() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "1.2.3.4".parse().unwrap());
        headers.insert("x-real-ip", "5.6.7.8".parse().unwrap());
        let fallback: IpAddr = "127.0.0.1".parse().unwrap();
        let ip = HttpHelper::client_ip(&headers, fallback);
        assert_eq!(ip.to_string(), "1.2.3.4");
    }

    #[test]
    fn test_ip_fallback() {
        let headers = HeaderMap::new();
        let fallback: IpAddr = "127.0.0.1".parse().unwrap();
        let ip = HttpHelper::client_ip(&headers, fallback);
        assert_eq!(ip.to_string(), "127.0.0.1");
    }

    #[test]
    fn test_ip_from_socket() {
        let headers = HeaderMap::new();
        let socket: SocketAddr = "192.168.1.1:8080".parse().unwrap();
        let ip = HttpHelper::client_ip_from_socket(&headers, &socket);
        assert_eq!(ip, "192.168.1.1");
    }

    // ─── Header helpers ───

    #[test]
    fn test_get_header() {
        let mut headers = HeaderMap::new();
        headers.insert("x-custom", "value123".parse().unwrap());
        assert_eq!(
            HttpHelper::get_header(&headers, "x-custom").as_deref(),
            Some("value123")
        );
        assert!(HttpHelper::get_header(&headers, "x-missing").is_none());
    }

    #[test]
    fn test_bearer_token() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer abc123xyz".parse().unwrap());
        assert_eq!(
            HttpHelper::bearer_token(&headers).as_deref(),
            Some("abc123xyz")
        );
    }

    #[test]
    fn test_bearer_token_missing() {
        let headers = HeaderMap::new();
        assert!(HttpHelper::bearer_token(&headers).is_none());
    }

    #[test]
    fn test_bearer_token_wrong_scheme() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Basic abc123".parse().unwrap());
        assert!(HttpHelper::bearer_token(&headers).is_none());
    }

    #[test]
    fn test_accept_language() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "accept-language",
            "fr-FR,fr;q=0.9,en-US;q=0.8".parse().unwrap(),
        );
        assert_eq!(
            HttpHelper::accept_language(&headers).as_deref(),
            Some("fr-FR")
        );
    }

    #[test]
    fn test_accepts_json() {
        let mut headers = HeaderMap::new();
        headers.insert("accept", "application/json".parse().unwrap());
        assert!(HttpHelper::accepts_json(&headers));

        let headers2 = HeaderMap::new();
        assert!(!HttpHelper::accepts_json(&headers2));
    }

    #[test]
    fn test_is_xhr() {
        let mut headers = HeaderMap::new();
        headers.insert("x-requested-with", "XMLHttpRequest".parse().unwrap());
        assert!(HttpHelper::is_xhr(&headers));

        let headers2 = HeaderMap::new();
        assert!(!HttpHelper::is_xhr(&headers2));
    }

    #[test]
    fn test_referer() {
        let mut headers = HeaderMap::new();
        headers.insert("referer", "https://example.com/page".parse().unwrap());
        assert_eq!(
            HttpHelper::referer(&headers).as_deref(),
            Some("https://example.com/page")
        );
    }
}
