use axum::{
    extract::ws::{WebSocket, WebSocketUpgrade},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use std::future::Future;

/// Default WebSocket message/frame size cap (64 KiB) applied by the framework helpers,
/// so a single client can't send arbitrarily large frames.
pub const WS_MAX_MESSAGE_SIZE: usize = 64 * 1024;

/// Returns true if the `Origin` header is absent (non-browser client) or its host matches
/// one of `allowed`. Use this to reject Cross-Site WebSocket Hijacking, since browsers do
/// **not** apply the same-origin policy to WebSocket upgrades.
pub fn origin_allowed(headers: &HeaderMap, allowed: &[&str]) -> bool {
    match headers.get("origin").and_then(|v| v.to_str().ok()) {
        None => true, // non-browser client (curl, server-to-server) — no Origin to forge
        Some(origin) => {
            let host = origin.split("://").nth(1).unwrap_or(origin);
            let host = host.split(['/', '?', '#']).next().unwrap_or(host);
            allowed
                .iter()
                .any(|a| a.eq_ignore_ascii_case(origin) || a.eq_ignore_ascii_case(host))
        }
    }
}

fn with_limits(ws: WebSocketUpgrade) -> WebSocketUpgrade {
    ws.max_message_size(WS_MAX_MESSAGE_SIZE)
        .max_frame_size(WS_MAX_MESSAGE_SIZE)
}

/// Like [`websocket_handler`] but rejects the upgrade (403) unless the request `Origin`
/// is in `allowed` (or absent). Prevents Cross-Site WebSocket Hijacking.
pub fn websocket_handler_checked<F, Fut>(
    ws: WebSocketUpgrade,
    headers: &HeaderMap,
    allowed: &[&str],
    handler: F,
) -> Response
where
    F: FnOnce(WebSocket) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    if !origin_allowed(headers, allowed) {
        return (StatusCode::FORBIDDEN, "Cross-site WebSocket rejected").into_response();
    }
    with_limits(ws).on_upgrade(handler)
}

/// Helper to create a WebSocket handler from a simple async function.
///
/// ```ignore
/// use karbon::ws::websocket_handler;
///
/// async fn handle_socket(mut socket: WebSocket) {
///     while let Some(Ok(msg)) = socket.recv().await {
///         if let Message::Text(text) = msg {
///             socket.send(Message::Text(format!("Echo: {}", text))).await.ok();
///         }
///     }
/// }
///
/// // In your router:
/// Router::new()
///     .route("/ws", get(|ws: WebSocketUpgrade| websocket_handler(ws, handle_socket)))
/// ```
pub fn websocket_handler<F, Fut>(ws: WebSocketUpgrade, handler: F) -> Response
where
    F: FnOnce(WebSocket) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    with_limits(ws).on_upgrade(handler)
}

/// Helper to create a WebSocket handler with shared state.
///
/// ```ignore
/// async fn chat_socket(mut socket: WebSocket, state: AppState) {
///     // access state.db, state.config, etc.
/// }
///
/// Router::new()
///     .route("/ws/chat", get(|ws: WebSocketUpgrade, State(state): State<AppState>| {
///         websocket_handler_with_state(ws, state, chat_socket)
///     }))
/// ```
pub fn websocket_handler_with_state<S, F, Fut>(
    ws: WebSocketUpgrade,
    state: S,
    handler: F,
) -> Response
where
    S: Send + 'static,
    F: FnOnce(WebSocket, S) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    with_limits(ws).on_upgrade(move |socket| handler(socket, state))
}
