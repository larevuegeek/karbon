use axum::{
    extract::ws::{WebSocket, WebSocketUpgrade},
    response::Response,
};
use std::future::Future;

/// Helper to create a WebSocket handler from a simple async function.
///
/// ```ignore
/// use framework::ws::websocket_handler;
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
    ws.on_upgrade(handler)
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
pub fn websocket_handler_with_state<S, F, Fut>(ws: WebSocketUpgrade, state: S, handler: F) -> Response
where
    S: Send + 'static,
    F: FnOnce(WebSocket, S) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    ws.on_upgrade(move |socket| handler(socket, state))
}
