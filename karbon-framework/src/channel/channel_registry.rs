use axum::extract::ws::{Message, WebSocket};
use futures::{SinkExt, StreamExt};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::{RwLock, broadcast};

/// Internal message routed through the broadcast channel
#[derive(Debug, Clone)]
struct ChannelMessage {
    channel: String,
    event: String,
    payload: String,
    sender_id: Option<u64>,
}

/// Wire format for client ↔ server WebSocket messages
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
struct WireMessage {
    channel: String,
    event: String,
    #[serde(default)]
    payload: serde_json::Value,
}

/// Manages WebSocket channels/rooms with typed messages.
///
/// ```ignore
/// use karbon::channel::ChannelRegistry;
/// use karbon::http::ws::websocket_handler_with_state;
///
/// let channels = ChannelRegistry::new();
///
/// // Mount in router (uses the existing ws helper)
/// Router::new()
///     .route("/ws/channels", get(|ws: WebSocketUpgrade, State(ch): State<ChannelRegistry>| {
///         websocket_handler_with_state(ws, ch, |socket, ch| ch.handle_socket(socket))
///     }));
///
/// // Broadcast from anywhere (controller, job, event handler)
/// channels.broadcast("chat/room-1", "new_message", &msg).await;
/// ```
#[derive(Clone)]
pub struct ChannelRegistry {
    tx: broadcast::Sender<ChannelMessage>,
    rooms: Arc<RwLock<HashMap<String, HashSet<u64>>>>,
    next_client_id: Arc<std::sync::atomic::AtomicU64>,
}

impl ChannelRegistry {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(1024);
        Self {
            tx,
            rooms: Arc::new(RwLock::new(HashMap::new())),
            next_client_id: Arc::new(std::sync::atomic::AtomicU64::new(1)),
        }
    }

    /// Broadcast a typed message to all clients subscribed to a channel
    pub async fn broadcast<T: Serialize>(&self, channel: &str, event: &str, data: &T) {
        let payload = match serde_json::to_string(data) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(channel = %channel, error = %e, "Failed to serialize channel message");
                return;
            }
        };
        let _ = self.tx.send(ChannelMessage {
            channel: channel.to_string(),
            event: event.to_string(),
            payload,
            sender_id: None,
        });
    }

    /// Broadcast a raw JSON value to a channel
    pub async fn broadcast_raw(&self, channel: &str, event: &str, payload: serde_json::Value) {
        let _ = self.tx.send(ChannelMessage {
            channel: channel.to_string(),
            event: event.to_string(),
            payload: payload.to_string(),
            sender_id: None,
        });
    }

    /// Get the number of connected clients in a channel
    pub async fn client_count(&self, channel: &str) -> usize {
        self.rooms
            .read()
            .await
            .get(channel)
            .map(|s| s.len())
            .unwrap_or(0)
    }

    /// Get all active channel names
    pub async fn active_channels(&self) -> Vec<String> {
        self.rooms.read().await.keys().cloned().collect()
    }

    /// Handle a WebSocket connection with the channel protocol.
    ///
    /// The client sends JSON messages:
    /// - Join:    `{"channel": "chat/room-1", "event": "join", "payload": {}}`
    /// - Leave:   `{"channel": "chat/room-1", "event": "leave", "payload": {}}`
    /// - Message: `{"channel": "chat/room-1", "event": "message", "payload": {"text": "hello"}}`
    pub async fn handle_socket(self, socket: WebSocket) {
        let client_id = self
            .next_client_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut rx = self.tx.subscribe();
        let (mut ws_tx, mut ws_rx) = socket.split();

        let subscribed: Arc<RwLock<HashSet<String>>> = Arc::new(RwLock::new(HashSet::new()));

        let sub_read = subscribed.clone();

        let mut send_task = tokio::spawn(async move {
            while let Ok(msg) = rx.recv().await {
                if msg.sender_id == Some(client_id) {
                    continue;
                }

                let subs = sub_read.read().await;
                if !subs.contains(&msg.channel) {
                    continue;
                }
                drop(subs);

                let wire = serde_json::json!({
                    "channel": msg.channel,
                    "event": msg.event,
                    "payload": serde_json::from_str::<serde_json::Value>(&msg.payload).unwrap_or_default(),
                });

                if ws_tx
                    .send(Message::Text(wire.to_string().into()))
                    .await
                    .is_err()
                {
                    break;
                }
            }
        });

        let sub_write = subscribed.clone();
        let rooms = self.rooms.clone();
        let tx = self.tx.clone();

        // Per-connection abuse limits (DoS mitigation).
        const MAX_MSG_BYTES: usize = 64 * 1024;
        const MAX_CHANNELS_PER_CONN: usize = 100;
        const MAX_CHANNEL_NAME: usize = 128;

        let mut recv_task = tokio::spawn(async move {
            while let Some(Ok(msg)) = ws_rx.next().await {
                let text = match msg {
                    Message::Text(t) => t.to_string(),
                    Message::Close(_) => break,
                    _ => continue,
                };

                // Drop oversized frames instead of parsing them.
                if text.len() > MAX_MSG_BYTES {
                    continue;
                }

                let Ok(wire) = serde_json::from_str::<WireMessage>(&text) else {
                    continue;
                };

                // Reject implausible channel names.
                if wire.channel.len() > MAX_CHANNEL_NAME {
                    continue;
                }

                match wire.event.as_str() {
                    "join" => {
                        {
                            let mut subs = sub_write.write().await;
                            // Cap the number of channels a single connection can join.
                            if !subs.contains(&wire.channel) && subs.len() >= MAX_CHANNELS_PER_CONN
                            {
                                continue;
                            }
                            subs.insert(wire.channel.clone());
                        }
                        rooms
                            .write()
                            .await
                            .entry(wire.channel.clone())
                            .or_default()
                            .insert(client_id);
                    }
                    "leave" => {
                        sub_write.write().await.remove(&wire.channel);
                        let mut rooms = rooms.write().await;
                        if let Some(set) = rooms.get_mut(&wire.channel) {
                            set.remove(&client_id);
                            if set.is_empty() {
                                rooms.remove(&wire.channel);
                            }
                        }
                    }
                    _ => {
                        let subs = sub_write.read().await;
                        if subs.contains(&wire.channel) {
                            let _ = tx.send(ChannelMessage {
                                channel: wire.channel,
                                event: wire.event,
                                payload: wire.payload.to_string(),
                                sender_id: Some(client_id),
                            });
                        }
                    }
                }
            }
        });

        tokio::select! {
            _ = &mut send_task => recv_task.abort(),
            _ = &mut recv_task => send_task.abort(),
        }

        // Cleanup: remove client from all rooms
        let subs = subscribed.read().await;
        let mut rooms = self.rooms.write().await;
        for channel in subs.iter() {
            if let Some(set) = rooms.get_mut(channel) {
                set.remove(&client_id);
                if set.is_empty() {
                    rooms.remove(channel);
                }
            }
        }
    }
}

impl Default for ChannelRegistry {
    fn default() -> Self {
        Self::new()
    }
}
