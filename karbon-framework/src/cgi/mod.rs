//! CGI adapter (feature `cgi`) — run a Karbon router on shared hosting.
//!
//! Classic CGI invokes the binary **once per request**, passing the request via
//! environment variables + stdin and reading the response from stdout. This is
//! the lowest-common-denominator way to run on a mutualisé host that supports
//! CGI but not a long-lived process.
//!
//! ⚠️ **Best-effort / unproven**: one process per request is slow, and this has
//! not been validated against a real cPanel/LiteSpeed host yet. Prefer a VPS,
//! Docker or PaaS for production (see the roadmap). FastCGI (a persistent
//! variant) is a future PoC.
//!
//! ```ignore
//! // In a small CGI entrypoint binary:
//! #[tokio::main(flavor = "current_thread")]
//! async fn main() {
//!     let router = build_router().with_state(state);
//!     karbon::cgi::serve_cgi(router).await.unwrap();
//! }
//! ```

use std::io::{Read, Write};

use axum::Router;
use axum::body::Body;
use axum::http::{HeaderName, HeaderValue, Method, Request, Response};
use tower::ServiceExt;

/// Build an `http::Request` from the CGI environment + stdin, run it through the
/// `router` once, and write the response to stdout in CGI format.
pub async fn serve_cgi(router: Router) -> std::io::Result<()> {
    let request = build_request();
    let response = router
        .oneshot(request)
        .await
        .unwrap_or_else(|_| Response::new(Body::empty()));
    write_response(response).await
}

fn env(key: &str) -> Option<String> {
    std::env::var(key).ok()
}

fn build_request() -> Request<Body> {
    let method = env("REQUEST_METHOD")
        .and_then(|m| Method::from_bytes(m.as_bytes()).ok())
        .unwrap_or(Method::GET);

    // PATH_INFO + QUERY_STRING (fall back to REQUEST_URI).
    let path = env("PATH_INFO")
        .or_else(|| env("SCRIPT_URL"))
        .or_else(|| env("REQUEST_URI"))
        .unwrap_or_else(|| "/".to_string());
    let path = path.split('?').next().unwrap_or("/").to_string();
    let uri = match env("QUERY_STRING") {
        Some(q) if !q.is_empty() => format!("{path}?{q}"),
        _ => path,
    };

    // Read the body (CONTENT_LENGTH bytes) from stdin.
    let len: usize = env("CONTENT_LENGTH")
        .and_then(|l| l.parse().ok())
        .unwrap_or(0);
    let mut body = vec![0u8; len];
    if len > 0 {
        let _ = std::io::stdin().read_exact(&mut body);
    }

    let mut builder = Request::builder().method(method).uri(uri);

    if let Some(ct) = env("CONTENT_TYPE") {
        builder = builder.header("content-type", ct);
    }
    // CGI passes request headers as HTTP_<UPPER_SNAKE>.
    for (k, v) in std::env::vars() {
        if let Some(name) = k.strip_prefix("HTTP_") {
            let header = name.to_lowercase().replace('_', "-");
            if let (Ok(n), Ok(val)) = (
                HeaderName::from_bytes(header.as_bytes()),
                HeaderValue::from_str(&v),
            ) {
                builder = builder.header(n, val);
            }
        }
    }

    builder.body(Body::from(body)).unwrap_or_else(|_| {
        Request::builder()
            .body(Body::empty())
            .expect("empty request")
    })
}

async fn write_response(response: Response<Body>) -> std::io::Result<()> {
    let (parts, body) = response.into_parts();
    let bytes = axum::body::to_bytes(body, usize::MAX)
        .await
        .unwrap_or_default();

    let status = parts.status;
    let reason = status.canonical_reason().unwrap_or("");

    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    // CGI uses a `Status:` header instead of an HTTP status line.
    write!(out, "Status: {} {}\r\n", status.as_u16(), reason)?;
    for (name, value) in parts.headers.iter() {
        if name == axum::http::header::CONTENT_LENGTH {
            continue;
        }
        if let Ok(v) = value.to_str() {
            write!(out, "{}: {}\r\n", name, v)?;
        }
    }
    write!(out, "Content-Length: {}\r\n\r\n", bytes.len())?;
    out.write_all(&bytes)?;
    out.flush()?;
    Ok(())
}
