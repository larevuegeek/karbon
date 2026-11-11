use std::sync::Arc;

use axum::extract::Request;
use axum::http::header::{ACCEPT, LOCATION};
use axum::http::{HeaderValue, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

/// Sends a browser that opens a protected page without a session to the login page,
/// instead of showing it the JSON of a `401`.
///
/// Only a page navigation is redirected — a `GET` that asks for HTML. An API call, a
/// `fetch`, a form submission keep their `401`: a script has to see the status, and
/// following a redirect would hand it the login page as if it were the answer.
///
/// The page asked for travels in `?next=`, for the login handler to return to (through
/// [`local_redirect`](crate::security::local_redirect), which only follows a local path).
///
/// [`App::serve`](crate::http::App) installs it when `LOGIN_PATH` is set.
pub async fn login_redirect(login_path: Arc<str>, request: Request, next: Next) -> Response {
    let wanted = wants_a_page(&request).then(|| {
        let uri = request.uri();
        uri.path_and_query()
            .map(|pq| pq.as_str().to_string())
            .unwrap_or_else(|| uri.path().to_string())
    });

    let response = next.run(request).await;

    match wanted {
        Some(target) if response.status() == StatusCode::UNAUTHORIZED && !is_login_page(&target, &login_path) => {
            let location = format!("{login_path}?next={}", encode_query_value(&target));
            match HeaderValue::from_str(&location) {
                Ok(location) => (StatusCode::SEE_OTHER, [(LOCATION, location)]).into_response(),
                Err(_) => response,
            }
        }
        _ => response,
    }
}

/// Whether `value` can be a `LOGIN_PATH`: a path of this site, not something a browser
/// would read as another one.
pub fn is_valid_login_path(value: &str) -> bool {
    value.starts_with('/')
        && !value.starts_with("//")
        && !value.starts_with("/\\")
        && !value
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || c == '?' || c == '#')
}

/// A browser loading a page: `GET`, HTML accepted, and — when the browser says how it is
/// fetching — a navigation rather than a script's request.
fn wants_a_page(request: &Request) -> bool {
    let header = |name: &str| request.headers().get(name).and_then(|v| v.to_str().ok());

    request.method() == Method::GET
        && request
            .headers()
            .get(ACCEPT)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|accept| accept.contains("text/html"))
        && header("sec-fetch-mode").is_none_or(|mode| mode.eq_ignore_ascii_case("navigate"))
}

fn is_login_page(target: &str, login_path: &str) -> bool {
    target.split('?').next() == Some(login_path)
}

/// Percent-encodes everything but unreserved characters and `/`, so the whole target —
/// its own `?` and `&` included — stays one query value.
fn encode_query_value(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => out.push(byte as char),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::body::Body;
    use axum::routing::get;
    use tower::ServiceExt;

    fn app() -> Router {
        let login: Arc<str> = Arc::from("/login");
        Router::new()
            .route(
                "/admin/posts",
                get(|| async { StatusCode::UNAUTHORIZED }).post(|| async { StatusCode::UNAUTHORIZED }),
            )
            .route("/login", get(|| async { StatusCode::UNAUTHORIZED }))
            .route("/public", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn(move |request: Request, next: Next| {
                login_redirect(login.clone(), request, next)
            }))
    }

    async fn call(method: &str, uri: &str, headers: &[(&str, &str)]) -> (StatusCode, Option<String>) {
        let mut request = axum::http::Request::builder().method(method).uri(uri);
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        let response = app().oneshot(request.body(Body::empty()).unwrap()).await.unwrap();
        let location = response.headers().get(LOCATION).map(|v| v.to_str().unwrap().to_string());
        (response.status(), location)
    }

    const BROWSER: (&str, &str) = ("accept", "text/html,application/xhtml+xml,*/*;q=0.8");

    #[tokio::test]
    async fn a_browser_without_a_session_is_sent_to_the_login_page() {
        let (status, location) = call("GET", "/admin/posts?page=2&sort=id", &[BROWSER]).await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        assert_eq!(location.as_deref(), Some("/login?next=/admin/posts%3Fpage%3D2%26sort%3Did"));

        let navigate = [BROWSER, ("sec-fetch-mode", "navigate")];
        assert_eq!(call("GET", "/admin/posts", &navigate).await.0, StatusCode::SEE_OTHER);
    }

    /// A script must see the 401 itself, not the login page behind a redirect.
    #[tokio::test]
    async fn scripts_and_api_clients_keep_their_401() {
        for headers in [vec![("accept", "application/json")], vec![], vec![BROWSER, ("sec-fetch-mode", "cors")]] {
            let (status, location) = call("GET", "/admin/posts", &headers).await;
            assert_eq!((status, location), (StatusCode::UNAUTHORIZED, None), "{headers:?}");
        }
        assert_eq!(call("POST", "/admin/posts", &[BROWSER]).await.0, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn the_login_page_itself_never_redirects_to_itself() {
        assert_eq!(call("GET", "/login", &[BROWSER]).await.0, StatusCode::UNAUTHORIZED);
        assert_eq!(call("GET", "/login?next=/x", &[BROWSER]).await.0, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn other_responses_pass_through() {
        assert_eq!(call("GET", "/public", &[BROWSER]).await, (StatusCode::OK, None));
        assert_eq!(call("GET", "/missing", &[BROWSER]).await.0, StatusCode::NOT_FOUND);
    }

    #[test]
    fn a_login_path_is_a_local_path() {
        for ok in ["/login", "/auth/sign-in", "/"] {
            assert!(is_valid_login_path(ok), "{ok}");
        }
        for bad in
            ["", "login", "//evil.test/login", "/\\evil.test", "https://evil.test", "/login?x=1", "/log in", "/a#b"]
        {
            assert!(!is_valid_login_path(bad), "{bad:?}");
        }
    }
}
