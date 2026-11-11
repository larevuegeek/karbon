//! Browser sessions: the access token carried by an `HttpOnly` cookie.
//!
//! An API client sends `Authorization: Bearer <token>`. A browser opening a server-rendered
//! page cannot: it only sends cookies. [`AuthGuard`](super::AuthGuard) accepts both, so the
//! same role checks protect an API and an HTML back-office.
//!
//! ```ignore
//! // After checking the password:
//! let token = jwt.generate_full(&user.email, &user.email, roles, Some(user.id), None, None)?;
//! let cookie = login_cookie(&token, 8 * 3600, is_https(&headers));
//! Ok(([(SET_COOKIE, cookie)], Redirect::to("/")))
//! ```
//!
//! The cookie is `HttpOnly` (scripts cannot read it) and `SameSite=Lax` (another site cannot
//! make the browser send it with a POST). The CSRF middleware — on by default — adds the
//! Origin check and the double-submit token on top: keep it enabled for cookie sessions.

use axum::http::HeaderMap;
use axum::http::header::{AUTHORIZATION, COOKIE};

/// Name of the cookie that carries the access token of a browser session.
pub const AUTH_COOKIE: &str = "karbon_auth";

/// The `Set-Cookie` value that opens a browser session for `max_age_secs`.
///
/// `secure` restricts the cookie to HTTPS: pass [`is_https`] for the request at hand.
pub fn login_cookie(token: &str, max_age_secs: i64, secure: bool) -> String {
    let secure = if secure { "; Secure" } else { "" };
    format!("{AUTH_COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age_secs}{secure}")
}

/// The `Set-Cookie` value that closes the browser session.
pub fn logout_cookie(secure: bool) -> String {
    login_cookie("", 0, secure)
}

/// Name of the cookie that carries the refresh token of a browser session.
pub const REFRESH_COOKIE: &str = "karbon_refresh";

/// The `Set-Cookie` value for the refresh token that renews a browser session.
///
/// The access token is short-lived; this one lasts (`max_age_secs`) and buys a new access
/// token when the old one has expired. `path` limits where the browser sends it: give the
/// prefix of the routes that renew or close the session (`/auth`), so it does not travel
/// with every request.
pub fn refresh_cookie(token: &str, max_age_secs: i64, path: &str, secure: bool) -> String {
    let secure = if secure { "; Secure" } else { "" };
    format!("{REFRESH_COOKIE}={token}; Path={path}; HttpOnly; SameSite=Lax; Max-Age={max_age_secs}{secure}")
}

/// The `Set-Cookie` value that removes the refresh token (same `path` as it was set with).
pub fn clear_refresh_cookie(path: &str, secure: bool) -> String {
    refresh_cookie("", 0, path, secure)
}

/// The refresh token a browser sent, if any.
pub fn refresh_token_from(headers: &HeaderMap) -> Option<&str> {
    cookie(headers, REFRESH_COOKIE)
}

/// The non-empty value of the cookie `name`, across every `Cookie` header.
fn cookie<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get_all(COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|cookies| cookies.split(';'))
        .find_map(|cookie| cookie.trim().strip_prefix(name)?.strip_prefix('='))
        .filter(|value| !value.is_empty())
}

/// Whether the browser reached the application over HTTPS, directly or through a
/// TLS-terminating proxy that says so in `X-Forwarded-Proto`.
///
/// Only used to decide the `Secure` flag: a client lying about it makes its own cookie
/// stricter, nothing more.
pub fn is_https(headers: &HeaderMap) -> bool {
    headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.eq_ignore_ascii_case("https"))
}

/// Why no token could be read from a request.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum NoToken {
    /// Neither an `Authorization` header nor a session cookie.
    Missing,
    /// An `Authorization` header that is not `Bearer <token>`.
    Malformed,
}

/// The access token of a request: the `Authorization: Bearer` header when there is one,
/// otherwise the session cookie.
///
/// A present but malformed `Authorization` header is an error, not a reason to fall back
/// on the cookie: the client said how it authenticates.
pub(crate) fn token_from(headers: &HeaderMap) -> Result<&str, NoToken> {
    if let Some(value) = headers.get(AUTHORIZATION) {
        return value
            .to_str()
            .ok()
            .and_then(|v| v.strip_prefix("Bearer "))
            .filter(|token| !token.is_empty())
            .ok_or(NoToken::Malformed);
    }

    cookie(headers, AUTH_COOKIE).ok_or(NoToken::Missing)
}

/// Where to send the browser after login: `next` when it is a path of this site,
/// `fallback` otherwise.
///
/// `next` comes from the query string, so anyone can craft a login link carrying it. Only a
/// path is accepted — `/admin/posts?page=2` — never something a browser would read as
/// another site (`https://evil.test`, `//evil.test`, `/\evil.test`).
pub fn local_redirect(next: Option<&str>, fallback: &str) -> String {
    match next {
        Some(path)
            if path.starts_with('/')
                && !path.starts_with("//")
                && !path.starts_with("/\\")
                && !path.chars().any(|c| c.is_control()) =>
        {
            path.to_string()
        }
        _ => fallback.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for (name, value) in pairs {
            headers.append(*name, value.parse().unwrap());
        }
        headers
    }

    #[test]
    fn the_bearer_header_is_read_first() {
        let h = headers(&[("authorization", "Bearer abc"), ("cookie", "karbon_auth=from-cookie")]);
        assert_eq!(token_from(&h), Ok("abc"));
    }

    #[test]
    fn the_session_cookie_stands_in_for_a_missing_header() {
        assert_eq!(token_from(&headers(&[("cookie", "karbon_auth=tok")])), Ok("tok"));
        assert_eq!(token_from(&headers(&[("cookie", "csrf_token=x; karbon_auth=tok; theme=dark")])), Ok("tok"));
        // Several Cookie headers (HTTP/2 may split them).
        assert_eq!(token_from(&headers(&[("cookie", "a=1"), ("cookie", "karbon_auth=tok")])), Ok("tok"));
    }

    #[test]
    fn another_cookie_sharing_the_prefix_is_not_the_session() {
        assert_eq!(token_from(&headers(&[("cookie", "karbon_auth_old=tok")])), Err(NoToken::Missing));
        assert_eq!(token_from(&headers(&[("cookie", "xkarbon_auth=tok")])), Err(NoToken::Missing));
    }

    #[test]
    fn nothing_or_an_emptied_cookie_is_missing() {
        assert_eq!(token_from(&headers(&[])), Err(NoToken::Missing));
        // What `logout_cookie` leaves behind until the browser drops it.
        assert_eq!(token_from(&headers(&[("cookie", "karbon_auth=")])), Err(NoToken::Missing));
    }

    /// A client that sends `Authorization` chose its scheme: a broken one is refused even
    /// when a valid session cookie rides along.
    #[test]
    fn a_malformed_authorization_header_does_not_fall_back_on_the_cookie() {
        for value in ["Basic dXNlcjpwYXNz", "abc", "Bearer ", "bearer abc"] {
            let h = headers(&[("authorization", value), ("cookie", "karbon_auth=tok")]);
            assert_eq!(token_from(&h), Err(NoToken::Malformed), "{value}");
        }
    }

    #[test]
    fn the_cookie_is_http_only_lax_and_secure_on_https() {
        let plain = login_cookie("tok", 3600, false);
        assert_eq!(plain, "karbon_auth=tok; Path=/; HttpOnly; SameSite=Lax; Max-Age=3600");
        assert!(login_cookie("tok", 3600, true).ends_with("; Secure"));
        assert!(logout_cookie(false).starts_with("karbon_auth=; ") && logout_cookie(false).contains("Max-Age=0"));
    }

    #[test]
    fn the_refresh_cookie_is_scoped_to_the_routes_that_use_it() {
        assert_eq!(
            refresh_cookie("r3fresh", 2_592_000, "/auth", true),
            "karbon_refresh=r3fresh; Path=/auth; HttpOnly; SameSite=Lax; Max-Age=2592000; Secure"
        );
        assert_eq!(
            clear_refresh_cookie("/auth", false),
            "karbon_refresh=; Path=/auth; HttpOnly; SameSite=Lax; Max-Age=0"
        );

        let h = headers(&[("cookie", "karbon_auth=access; karbon_refresh=r3fresh")]);
        assert_eq!(refresh_token_from(&h), Some("r3fresh"));
        assert_eq!(token_from(&h), Ok("access"));
        assert_eq!(refresh_token_from(&headers(&[("cookie", "karbon_auth=access")])), None);
        assert_eq!(refresh_token_from(&headers(&[("cookie", "karbon_refresh=")])), None);
    }

    #[test]
    fn https_is_read_from_the_forwarded_proto() {
        assert!(is_https(&headers(&[("x-forwarded-proto", "https")])));
        assert!(is_https(&headers(&[("x-forwarded-proto", "HTTPS")])));
        assert!(!is_https(&headers(&[("x-forwarded-proto", "http")])));
        assert!(!is_https(&headers(&[])));
    }

    /// `?next=` is attacker-controlled: only a path of this site may be followed.
    #[test]
    fn only_a_local_path_is_followed_after_login() {
        assert_eq!(local_redirect(Some("/admin/posts?page=2"), "/"), "/admin/posts?page=2");
        assert_eq!(local_redirect(Some("/"), "/home"), "/");
        for hostile in [
            "https://evil.test", "//evil.test", "/\\evil.test", "evil.test", "javascript:alert(1)", "",
            "/ok\r\nSet-Cookie: x=1",
        ] {
            assert_eq!(local_redirect(Some(hostile), "/home"), "/home", "{hostile:?}");
        }
        assert_eq!(local_redirect(None, "/home"), "/home");
    }
}
