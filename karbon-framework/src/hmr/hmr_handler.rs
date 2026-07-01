use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::Response;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;

/// Watches files and notifies connected browsers via WebSocket.
///
/// ```ignore
/// use karbon::hmr::HmrServer;
///
/// let hmr = HmrServer::new()
///     .watch("templates/")
///     .watch("static/");
///
/// // Mount the HMR endpoints
/// let app = Router::new()
///     .route("/_hmr/ws", get({
///         let hmr = hmr.clone();
///         move |ws: WebSocketUpgrade| hmr.ws_handler(ws)
///     }))
///     .layer(hmr.inject_script_layer());
///
/// // Start watching (spawns a background task)
/// hmr.start();
/// ```
#[derive(Clone)]
pub struct HmrServer {
    tx: broadcast::Sender<HmrEvent>,
    watch_paths: Arc<Vec<PathBuf>>,
}

#[derive(Debug, Clone)]
enum HmrEvent {
    /// A CSS file changed — hot-swap without reload
    CssChanged { path: String },
    /// Any other file changed — trigger full reload
    FullReload { path: String },
}

impl HmrServer {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(64);
        Self {
            tx,
            watch_paths: Arc::new(Vec::new()),
        }
    }

    /// Add a directory to watch for changes
    pub fn watch(mut self, path: &str) -> Self {
        Arc::get_mut(&mut self.watch_paths)
            .expect("watch() must be called before start()")
            .push(PathBuf::from(path));
        self
    }

    /// Start watching files in a background task.
    /// Uses polling (checks every 500ms) for maximum cross-platform compatibility.
    pub fn start(&self) {
        let tx = self.tx.clone();
        let paths = self.watch_paths.clone();

        tokio::spawn(async move {
            let mut last_modified = std::collections::HashMap::new();
            let mut scan_count = 0u64;

            loop {
                let mut current_paths = std::collections::HashSet::new();

                for watch_path in paths.iter() {
                    if let Ok(entries) = walk_dir(watch_path) {
                        for entry in entries {
                            let modified = entry.metadata().ok().and_then(|m| m.modified().ok());

                            let Some(modified) = modified else { continue };
                            let path_str = entry.path().display().to_string();
                            current_paths.insert(path_str.clone());

                            let changed = match last_modified.get(&path_str) {
                                Some(prev) => *prev != modified,
                                None => false, // first scan, don't trigger
                            };

                            last_modified.insert(path_str.clone(), modified);

                            if changed {
                                let event = if path_str.ends_with(".css") {
                                    HmrEvent::CssChanged { path: path_str }
                                } else {
                                    HmrEvent::FullReload { path: path_str }
                                };
                                let _ = tx.send(event);
                            }
                        }
                    }
                }

                // Purge deleted files every 20 scans (~10s) to prevent unbounded growth
                scan_count += 1;
                if scan_count.is_multiple_of(20) {
                    last_modified.retain(|k, _| current_paths.contains(k));
                }

                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        });
    }

    /// WebSocket handler for HMR clients
    pub async fn ws_handler(self, ws: WebSocketUpgrade) -> Response {
        ws.on_upgrade(move |socket| self.handle_ws(socket))
    }

    async fn handle_ws(self, mut socket: WebSocket) {
        let mut rx = self.tx.subscribe();

        loop {
            tokio::select! {
                msg = rx.recv() => {
                    match msg {
                        Ok(HmrEvent::CssChanged { path }) => {
                            let json = serde_json::json!({ "type": "css", "path": path });
                            if socket.send(Message::Text(json.to_string().into())).await.is_err() {
                                break;
                            }
                        }
                        Ok(HmrEvent::FullReload { path }) => {
                            let json = serde_json::json!({ "type": "reload", "path": path });
                            if socket.send(Message::Text(json.to_string().into())).await.is_err() {
                                break;
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(_) => break,
                    }
                }
                msg = socket.recv() => {
                    match msg {
                        Some(Ok(Message::Close(_))) | None => break,
                        _ => {}
                    }
                }
            }
        }
    }

    /// Returns the client-side script tag to inject into HTML pages in dev mode.
    ///
    /// ```ignore
    /// // In your HTML template or layout:
    /// if cfg!(debug_assertions) {
    ///     format!("{}\n{}", body_html, hmr.client_script())
    /// }
    /// ```
    pub fn client_script(&self) -> String {
        format!("<script>{}</script>", CLIENT_JS)
    }
}

impl Default for HmrServer {
    fn default() -> Self {
        Self::new()
    }
}

const MAX_WALK_DEPTH: usize = 10;
const MAX_WALK_FILES: usize = 10_000;

fn walk_dir(path: &std::path::Path) -> Result<Vec<std::fs::DirEntry>, std::io::Error> {
    let mut entries = Vec::new();
    walk_dir_inner(path, 0, &mut entries)?;
    Ok(entries)
}

fn walk_dir_inner(
    path: &std::path::Path,
    depth: usize,
    entries: &mut Vec<std::fs::DirEntry>,
) -> Result<(), std::io::Error> {
    if depth > MAX_WALK_DEPTH || entries.len() >= MAX_WALK_FILES || !path.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.') || name == "node_modules" || name == "target" {
                continue;
            }
            walk_dir_inner(&entry.path(), depth + 1, entries)?;
        } else if file_type.is_file() {
            entries.push(entry);
            if entries.len() >= MAX_WALK_FILES {
                return Ok(());
            }
        }
    }
    Ok(())
}

/// Client-side HMR runtime — connects to the dev server, hot-swaps CSS,
/// full-reloads for everything else.
const CLIENT_JS: &str = r#"
(function() {
    if (typeof window === 'undefined') return;
    const url = (location.protocol === 'https:' ? 'wss' : 'ws') + '://' + location.host + '/_hmr/ws';
    let ws;
    function connect() {
        ws = new WebSocket(url);
        ws.onmessage = function(e) {
            try {
                const msg = JSON.parse(e.data);
                if (msg.type === 'css') {
                    // Hot-swap CSS: reload all stylesheets
                    document.querySelectorAll('link[rel="stylesheet"]').forEach(function(link) {
                        const href = link.href.split('?')[0];
                        link.href = href + '?_hmr=' + Date.now();
                    });
                    console.log('[HMR] CSS updated:', msg.path);
                } else if (msg.type === 'reload') {
                    console.log('[HMR] Reloading:', msg.path);
                    location.reload();
                }
            } catch {}
        };
        ws.onclose = function() { setTimeout(connect, 2000); };
    }
    connect();
})();
"#;
