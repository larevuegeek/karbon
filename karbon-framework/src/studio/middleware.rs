use axum::body::Body;
use axum::extract::{ConnectInfo, Request};
use axum::http::header::{CONTENT_LENGTH, CONTENT_TYPE};
use axum::middleware::Next;
use axum::response::Response;
use std::net::SocketAddr;
use std::time::Instant;

use super::collector::StudioCollector;

/// Middleware that records every request/response into the StudioCollector.
/// Skips studio's own routes to avoid noise.
pub async fn studio_middleware(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path().to_string();

    // Don't record studio's own requests
    if path.starts_with("/_studio") {
        return next.run(request).await;
    }

    let method = request.method().to_string();
    let request_id = request
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    let request_headers: Vec<(String, String)> = request
        .headers()
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("<binary>").to_string()))
        .collect();

    let collector = request.extensions().get::<StudioCollector>().cloned();

    let start = Instant::now();
    let response = next.run(request).await;
    let duration = start.elapsed();

    if let Some(collector) = collector {
        let status = response.status().as_u16();
        let response_headers: Vec<(String, String)> = response
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("<binary>").to_string()))
            .collect();

        let remote = addr.to_string();

        tokio::spawn(async move {
            collector
                .record_request(
                    method,
                    path,
                    status,
                    duration,
                    request_headers,
                    response_headers,
                    request_id,
                    Some(remote),
                )
                .await;
        });
    }

    response
}

/// Middleware that injects a Symfony-style debug toolbar at the bottom of every
/// HTML response (dev only). Non-HTML responses pass through untouched.
pub async fn studio_toolbar_middleware(request: Request, next: Next) -> Response {
    let path = request.uri().path().to_string();
    if path.starts_with("/_studio") {
        return next.run(request).await;
    }
    let method = request.method().to_string();
    let token = request
        .extensions()
        .get::<super::StudioToken>()
        .map(|t| t.0.clone());

    let start = Instant::now();
    let response = next.run(request).await;
    let duration = start.elapsed();

    // Only inject for requests running in debug mode (local dev, or a live debug session).
    // Ordinary production traffic on a debug-configured binary never sees the toolbar.
    if !crate::http::dev_mode::active() {
        return response;
    }

    // Only touch HTML responses.
    let is_html = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|c| c.contains("text/html"))
        .unwrap_or(false);
    if !is_html {
        return response;
    }

    let status = response.status().as_u16();
    let request_id = response
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    let (mut parts, body) = response.into_parts();
    let bytes = match axum::body::to_bytes(body, usize::MAX).await {
        Ok(b) => b,
        Err(_) => return Response::from_parts(parts, Body::empty()),
    };
    let mut html = String::from_utf8_lossy(&bytes).into_owned();

    if let Some(pos) = html.rfind("</body>") {
        let toolbar = render_toolbar(
            &method,
            &path,
            status,
            duration.as_millis(),
            request_id,
            token.as_deref(),
        );
        html.insert_str(pos, &toolbar);
        // Body length changed — let the server recompute it.
        parts.headers.remove(CONTENT_LENGTH);
        Response::from_parts(parts, Body::from(html))
    } else {
        Response::from_parts(parts, Body::from(bytes))
    }
}

fn render_toolbar(
    method: &str,
    path: &str,
    status: u16,
    ms: u128,
    request_id: Option<String>,
    token: Option<&str>,
) -> String {
    let status_color = if (200..300).contains(&status) {
        "#3ddc84"
    } else if (300..400).contains(&status) {
        "#f5c518"
    } else {
        "#ff4d4d"
    };

    let version = super::VERSION;
    let env = std::env::var("APP_ENV").unwrap_or_else(|_| "development".to_string());
    let features = super::enabled_features().join(", ");
    let rid_full = request_id.clone().unwrap_or_else(|| "—".to_string());
    let rid_short: String = request_id
        .as_deref()
        .map(|r| r.chars().take(8).collect())
        .unwrap_or_default();

    let studio_link = match token {
        Some(t) => format!("/_studio?token={}", html_escape_attr(t)),
        None => "/_studio".to_string(),
    };

    // Drop-up info panel (Symfony-profiler-like), shown on hover of the karbon chip.
    let panel = format!(
        r#"<div class="kbt-panel">
      <div class="kbt-h">◆ Karbon framework</div>
      <div class="kbt-row"><span>Version</span><b>{version}</b></div>
      <div class="kbt-row"><span>Environment</span><b>{env}</b></div>
      <div class="kbt-row"><span>Features</span><b>{features}</b></div>
      <div class="kbt-h" style="margin-top:8px">Request</div>
      <div class="kbt-row"><span>Method</span><b>{method}</b></div>
      <div class="kbt-row"><span>Path</span><b>{path}</b></div>
      <div class="kbt-row"><span>Status</span><b style="color:{status_color}">{status}</b></div>
      <div class="kbt-row"><span>Duration</span><b>{ms} ms</b></div>
      <div class="kbt-row"><span>Request-Id</span><b>{rid_full}</b></div>
      <a class="kbt-open" href="{studio_link}" target="_blank" rel="noreferrer">Open Studio dashboard →</a>
      <a class="kbt-open" href="{studio_link}#terminal" target="_blank" rel="noreferrer">Terminal →</a>
    </div>"#,
        version = html_escape_attr(version),
        env = html_escape_attr(&env),
        features = html_escape_attr(&features),
        method = html_escape_attr(method),
        path = html_escape_attr(path),
        rid_full = html_escape_attr(&rid_full),
    );

    format!(
        r#"<style>
#karbon-toolbar{{position:fixed;bottom:0;left:0;right:0;z-index:2147483647;display:flex;gap:14px;align-items:center;height:30px;padding:0 12px;font:12px/30px ui-monospace,SFMono-Regular,Menlo,monospace;background:#08090f;color:#e6e8eb;border-top:1px solid #242a44;box-shadow:0 -2px 8px rgba(0,0,0,.4)}}
#karbon-toolbar a{{color:#a99bff;text-decoration:none}}
#karbon-toolbar .kbt-chip{{position:relative;display:flex;align-items:center;gap:8px;cursor:default}}
#karbon-toolbar .kbt-panel{{display:none;position:absolute;bottom:34px;left:0;min-width:340px;max-width:480px;padding:12px 14px;background:#11142a;color:#e6e8eb;border:1px solid #2a3052;border-radius:12px;box-shadow:0 10px 30px rgba(0,0,0,.55);line-height:1.5}}
#karbon-toolbar .kbt-chip:hover .kbt-panel{{display:block}}
#karbon-toolbar .kbt-h{{font-weight:700;color:#a99bff;margin-bottom:4px}}
#karbon-toolbar .kbt-row{{display:flex;justify-content:space-between;gap:16px}}
#karbon-toolbar .kbt-row span{{color:#8b92ad}}
#karbon-toolbar .kbt-row b{{font-weight:600;word-break:break-all;text-align:right}}
#karbon-toolbar .kbt-open{{display:inline-block;margin-top:10px;color:#a99bff}}
</style>
<div id="karbon-toolbar">
  <div class="kbt-chip">
    <span style="font-weight:700;color:#a99bff">◆ karbon</span>
    <span style="opacity:.8">v{version}</span>
    {panel}
  </div>
  <span><b style="color:#7aa2f7">{method}</b> {path}</span>
  <span style="color:{status_color}">{status}</span>
  <span style="opacity:.7">{ms} ms</span>
  <span style="opacity:.6">#{rid_short}</span>
  <a href="{studio_link}" target="_blank" rel="noreferrer" style="margin-left:auto">Studio →</a>
</div>"#,
        version = html_escape_attr(version),
        method = html_escape_attr(method),
        path = html_escape_attr(path),
        rid_short = html_escape_attr(&rid_short),
    )
}

fn html_escape_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::http::Request;
    use axum::response::{Html, Json};
    use axum::routing::get;
    use tower::ServiceExt;

    async fn body_string(res: Response) -> String {
        let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    #[tokio::test]
    async fn injects_toolbar_into_html() {
        let app = Router::new()
            .route("/", get(|| async { Html("<html><body>hi</body></html>") }))
            .layer(axum::middleware::from_fn(studio_toolbar_middleware));
        let res = crate::http::dev_mode::scoped(
            true,
            app.oneshot(Request::builder().uri("/").body(Body::empty()).unwrap()),
        )
        .await
        .unwrap();
        let html = body_string(res).await;
        assert!(html.contains("karbon-toolbar"), "toolbar not injected");
        assert!(html.contains("hi"), "original content lost");
        // Injected before </body>
        assert!(html.find("karbon-toolbar").unwrap() < html.find("</body>").unwrap());
    }

    #[tokio::test]
    async fn skips_non_html_responses() {
        let app = Router::new()
            .route(
                "/api",
                get(|| async { Json(serde_json::json!({"ok": true})) }),
            )
            .layer(axum::middleware::from_fn(studio_toolbar_middleware));
        let res = crate::http::dev_mode::scoped(
            true,
            app.oneshot(Request::builder().uri("/api").body(Body::empty()).unwrap()),
        )
        .await
        .unwrap();
        let body = body_string(res).await;
        assert!(!body.contains("karbon-toolbar"));
    }

    #[tokio::test]
    async fn skips_studio_routes() {
        let app = Router::new()
            .route(
                "/_studio",
                get(|| async { Html("<html><body>dash</body></html>") }),
            )
            .layer(axum::middleware::from_fn(studio_toolbar_middleware));
        let res = crate::http::dev_mode::scoped(
            true,
            app.oneshot(
                Request::builder()
                    .uri("/_studio")
                    .body(Body::empty())
                    .unwrap(),
            ),
        )
        .await
        .unwrap();
        let html = body_string(res).await;
        assert!(!html.contains("karbon-toolbar"));
    }
}
