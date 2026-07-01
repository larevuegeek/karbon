use axum::body::Body;
use axum::extract::Request;
use axum::http::{Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::Response;

/// HTTP caching middleware: adds an `ETag` to successful `GET` responses and
/// returns `304 Not Modified` when the client's `If-None-Match` matches.
///
/// The ETag is derived from the response body, so identical bodies stay cached
/// on the client without re-downloading.
pub async fn http_cache(request: Request, next: Next) -> Response {
    let is_get = request.method() == Method::GET;
    let if_none_match = request
        .headers()
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);

    let response = next.run(request).await;

    if !is_get || response.status() != StatusCode::OK {
        return response;
    }

    let (mut parts, body) = response.into_parts();
    let bytes = match axum::body::to_bytes(body, usize::MAX).await {
        Ok(b) => b,
        Err(_) => return Response::from_parts(parts, Body::empty()),
    };

    let etag = etag_for(&bytes);
    if let Ok(value) = etag.parse() {
        parts.headers.insert(header::ETAG, value);
    }

    // Client already has this exact representation.
    if if_none_match.as_deref() == Some(etag.as_str()) {
        parts.status = StatusCode::NOT_MODIFIED;
        parts.headers.remove(header::CONTENT_LENGTH);
        return Response::from_parts(parts, Body::empty());
    }

    Response::from_parts(parts, Body::from(bytes))
}

fn etag_for(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(16);
    for b in digest.iter().take(8) {
        hex.push_str(&format!("{b:02x}"));
    }
    format!("\"{hex}\"")
}
