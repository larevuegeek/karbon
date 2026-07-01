use axum::extract::ws::{Message, WebSocket};
use axum::response::{Html, IntoResponse, Response};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::live_component::LiveComponent;

/// Wire format for client → server events
#[derive(Debug, Deserialize)]
struct ClientEvent {
    event: String,
    #[serde(default)]
    params: HashMap<String, String>,
}

/// Wire format for server → client updates
#[derive(Debug, Serialize)]
struct ServerPatch {
    html: String,
}

/// Render a LiveComponent as an initial HTML page.
///
/// Returns the component HTML wrapped in a minimal shell with the livewire client JS.
/// The client connects via WebSocket and sends events back to the server.
///
/// ```ignore
/// use karbon::livewire::{LiveComponent, live_render, live_socket};
///
/// #[karbon::get("/counter")]
/// async fn counter_page() -> impl IntoResponse {
///     let component = Counter { count: 0 };
///     live_render(component, "/ws/counter")
/// }
///
/// // WebSocket handler for the live component
/// Router::new()
///     .route("/ws/counter", get(|ws: WebSocketUpgrade| {
///         ws.on_upgrade(|socket| live_socket(socket, Counter { count: 0 }))
///     }))
/// ```
pub fn live_render(component: impl LiveComponent, ws_url: &str) -> Response {
    let html = component.render();

    let page = format!(
        r#"<div id="lw-root">{html}</div>
<script>{CLIENT_JS}</script>
<script>LiveWire.connect("{ws_url}");</script>"#
    );

    Html(page).into_response()
}

/// Handle a WebSocket connection for a LiveComponent.
///
/// Receives events from the client, calls `handle_event`, re-renders, and sends
/// the updated HTML back.
pub async fn live_socket(socket: WebSocket, mut component: impl LiveComponent) {
    component.mount().await;

    let (mut tx, mut rx) = socket.split();

    // Send initial render
    let html = component.render();
    let patch = serde_json::to_string(&ServerPatch { html }).unwrap_or_default();
    let _ = tx.send(Message::Text(patch.into())).await;

    while let Some(Ok(msg)) = rx.next().await {
        let text = match msg {
            Message::Text(t) => t.to_string(),
            Message::Close(_) => break,
            _ => continue,
        };

        let Ok(event) = serde_json::from_str::<ClientEvent>(&text) else {
            continue;
        };

        component.handle_event(&event.event, &event.params).await;

        let html = component.render();
        let patch = serde_json::to_string(&ServerPatch { html }).unwrap_or_default();
        if tx.send(Message::Text(patch.into())).await.is_err() {
            break;
        }
    }
}

/// Minimal client-side JS runtime for LiveWire components.
/// Handles WebSocket connection, DOM patching, and event binding.
const CLIENT_JS: &str = r#"
const LiveWire = {
    ws: null,
    connect(url) {
        const wsUrl = (location.protocol === 'https:' ? 'wss' : 'ws') + '://' + location.host + url;
        this.ws = new WebSocket(wsUrl);
        this.ws.onmessage = (e) => {
            try {
                const patch = JSON.parse(e.data);
                if (patch.html) {
                    document.getElementById('lw-root').innerHTML = patch.html;
                    this.bind();
                }
            } catch {}
        };
        this.ws.onclose = () => setTimeout(() => this.connect(url), 2000);
        this.bind();
    },
    bind() {
        document.querySelectorAll('[lw-click]').forEach(el => {
            if (el._lw) return;
            el._lw = true;
            el.addEventListener('click', () => {
                const event = el.getAttribute('lw-click');
                const params = {};
                for (const attr of el.attributes) {
                    if (attr.name.startsWith('lw-param-')) {
                        params[attr.name.slice(9)] = attr.value;
                    }
                }
                this.send(event, params);
            });
        });
        document.querySelectorAll('[lw-submit]').forEach(el => {
            if (el._lw) return;
            el._lw = true;
            el.addEventListener('submit', (e) => {
                e.preventDefault();
                const event = el.getAttribute('lw-submit');
                const params = {};
                const formData = new FormData(el);
                for (const [k, v] of formData) params[k] = v;
                this.send(event, params);
            });
        });
        document.querySelectorAll('[lw-input]').forEach(el => {
            if (el._lw) return;
            el._lw = true;
            el.addEventListener('input', () => {
                const event = el.getAttribute('lw-input');
                this.send(event, { value: el.value });
            });
        });
    },
    send(event, params) {
        if (this.ws && this.ws.readyState === 1) {
            this.ws.send(JSON.stringify({ event, params: params || {} }));
        }
    }
};
"#;
