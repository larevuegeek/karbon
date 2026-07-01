use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, Query, Request, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::middleware::Next;
use axum::response::{Html, IntoResponse, Response};
use serde::Deserialize;
use std::net::SocketAddr;

use super::collector::StudioCollector;

/// Constant-time string comparison to avoid leaking the token via timing.
fn constant_time_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Authoritative gate for every `/_studio` route: allow only requests from the loopback
/// interface (dev convenience — the dashboard is meant to be local) OR requests carrying a
/// valid token. This fails closed for any network peer even when `APP_ENV` is unset, so the
/// terminal and DB browser are never reachable unauthenticated from the network.
pub async fn studio_guard(
    State(state): State<StudioState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    request: Request,
    next: Next,
) -> Response {
    let token_ok = {
        let query_token = request.uri().query().and_then(|q| {
            q.split('&')
                .find_map(|kv| kv.strip_prefix("token=").map(str::to_string))
        });
        let cookie_token = request
            .headers()
            .get("cookie")
            .and_then(|v| v.to_str().ok())
            .and_then(|c| {
                c.split(';')
                    .find_map(|p| p.trim().strip_prefix("_studio_token=").map(str::to_string))
            });
        query_token
            .as_deref()
            .is_some_and(|t| constant_time_eq(t, &state.token))
            || cookie_token
                .as_deref()
                .is_some_and(|t| constant_time_eq(t, &state.token))
    };

    // The loopback bypass is a *local dev* convenience only. In a release build the peer is
    // the reverse proxy (also loopback on a same-host deploy), so trusting it would expose
    // Studio to the whole internet. Release builds require a token or an active debug session.
    let loopback_ok = cfg!(debug_assertions) && peer.ip().is_loopback();

    if loopback_ok || token_ok || crate::http::dev_mode::active() {
        next.run(request).await
    } else {
        (
            StatusCode::UNAUTHORIZED,
            "Unauthorized — Studio requires a valid token or an active debug session",
        )
            .into_response()
    }
}

#[derive(Clone)]
pub struct StudioState {
    pub collector: StudioCollector,
    pub token: String,
    /// DB pool for schema introspection (None when no database is configured).
    pub db: Option<crate::db::DbPool>,
}

#[derive(Deserialize)]
pub struct TokenQuery {
    pub token: Option<String>,
}

/// In development the dashboard is dev-only and localhost-bound already, so we
/// don't require the random token — otherwise links that can't carry it (the
/// debug toolbar, the welcome page) would 403. Token enforcement stays on as
/// soon as `APP_ENV` is anything production-like.
fn studio_dev_open() -> bool {
    match std::env::var("APP_ENV") {
        Ok(v) => {
            let v = v.trim().to_lowercase();
            v.is_empty() || v.starts_with("dev") || v == "test"
        }
        Err(_) => true,
    }
}

fn check_token(state: &StudioState, query: &TokenQuery, headers: &HeaderMap) -> bool {
    // 0. Development: localhost-only dev dashboard — skip the token entirely.
    if studio_dev_open() {
        return true;
    }
    // 1. Check query param (initial access)
    if let Some(t) = &query.token
        && constant_time_eq(t, &state.token)
    {
        return true;
    }
    // 2. Check cookie (subsequent requests — avoids token in URLs/logs)
    if let Some(cookie) = headers.get("cookie")
        && let Ok(cookie_str) = cookie.to_str()
    {
        for part in cookie_str.split(';') {
            let part = part.trim();
            if let Some(val) = part.strip_prefix("_studio_token=")
                && constant_time_eq(val, &state.token)
            {
                return true;
            }
        }
    }
    false
}

/// Serve the main studio dashboard HTML — sets httpOnly cookie for subsequent requests
pub async fn dashboard(
    State(state): State<StudioState>,
    Query(query): Query<TokenQuery>,
    headers: HeaderMap,
) -> Response {
    if !check_token(&state, &query, &headers) {
        return (StatusCode::UNAUTHORIZED, "Unauthorized — token required").into_response();
    }

    let html = include_str!("assets/index.html").replace("{{TOKEN}}", &state.token);
    let secure = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.eq_ignore_ascii_case("https"));
    let cookie = format!(
        "_studio_token={}; Path=/_studio; HttpOnly; SameSite=Strict{}",
        state.token,
        if secure { "; Secure" } else { "" }
    );

    ([(header::SET_COOKIE, cookie)], Html(html)).into_response()
}

/// Serve CSS
pub async fn styles(
    State(state): State<StudioState>,
    Query(query): Query<TokenQuery>,
    headers: HeaderMap,
) -> Response {
    if !check_token(&state, &query, &headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    (
        [(header::CONTENT_TYPE, "text/css")],
        include_str!("assets/styles.css"),
    )
        .into_response()
}

/// Serve JS
pub async fn script(
    State(state): State<StudioState>,
    Query(query): Query<TokenQuery>,
    headers: HeaderMap,
) -> Response {
    if !check_token(&state, &query, &headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    (
        [(header::CONTENT_TYPE, "application/javascript")],
        include_str!("assets/app.js"),
    )
        .into_response()
}

/// JSON API: get all recorded data
pub async fn api_data(
    State(state): State<StudioState>,
    Query(query): Query<TokenQuery>,
    headers: HeaderMap,
) -> Response {
    if !check_token(&state, &query, &headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    let (requests, events, jobs, mails, stats) = tokio::join!(
        state.collector.get_requests(),
        state.collector.get_events(),
        state.collector.get_jobs(),
        state.collector.get_mails(),
        state.collector.get_stats(),
    );

    axum::Json(serde_json::json!({
        "requests": requests,
        "events": events,
        "jobs": jobs,
        "mails": mails,
        "stats": stats,
    }))
    .into_response()
}

/// JSON API: get stats only
pub async fn api_stats(
    State(state): State<StudioState>,
    Query(query): Query<TokenQuery>,
    headers: HeaderMap,
) -> Response {
    if !check_token(&state, &query, &headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    axum::Json(state.collector.get_stats().await).into_response()
}

/// JSON API: framework info (version, env, features) + detected DB tables.
pub async fn api_info(
    State(state): State<StudioState>,
    Query(query): Query<TokenQuery>,
    headers: HeaderMap,
) -> Response {
    if !check_token(&state, &query, &headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let tables = match &state.db {
        Some(pool) => list_tables(pool).await.unwrap_or_default(),
        None => Vec::new(),
    };
    axum::Json(serde_json::json!({
        "version": super::VERSION,
        "environment": std::env::var("APP_ENV").unwrap_or_else(|_| "development".to_string()),
        "features": super::enabled_features(),
        "database": state.db.is_some(),
        "driver": driver_name(),
        "tables": tables,
    }))
    .into_response()
}

/// The active database driver (compile-time feature).
fn driver_name() -> &'static str {
    if cfg!(feature = "postgres") {
        "postgres"
    } else if cfg!(feature = "sqlite") {
        "sqlite"
    } else {
        "mysql"
    }
}

#[derive(serde::Serialize)]
struct ColumnInfo {
    name: String,
    ty: String,
}

#[derive(serde::Serialize)]
struct TableInfo {
    name: String,
    rows: i64,
    columns: Vec<ColumnInfo>,
}

/// JSON API: schema browser — tables with row counts and columns (best-effort).
pub async fn api_database(
    State(state): State<StudioState>,
    Query(query): Query<TokenQuery>,
    headers: HeaderMap,
) -> Response {
    if !check_token(&state, &query, &headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let tables = match &state.db {
        Some(pool) => describe_database(pool).await,
        None => Vec::new(),
    };
    axum::Json(serde_json::json!({
        "driver": driver_name(),
        "connected": state.db.is_some(),
        "tables": tables,
    }))
    .into_response()
}

async fn describe_database(pool: &crate::db::DbPool) -> Vec<TableInfo> {
    let names = list_tables(pool).await.unwrap_or_default();
    let mut out = Vec::new();
    for name in names.into_iter().take(80) {
        // Only introspect plain identifiers (defence-in-depth before interpolation).
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            continue;
        }
        let rows = count_rows(pool, &name).await.unwrap_or(-1);
        let columns = table_columns(pool, &name).await.unwrap_or_default();
        out.push(TableInfo {
            name,
            rows,
            columns,
        });
    }
    out
}

/// `SELECT COUNT(*)` with a driver-quoted identifier (name pre-validated by caller).
async fn count_rows(pool: &crate::db::DbPool, table: &str) -> Option<i64> {
    let sql = if cfg!(feature = "mysql") {
        format!("SELECT COUNT(*) FROM `{table}`")
    } else {
        format!("SELECT COUNT(*) FROM \"{table}\"")
    };
    let (n,): (i64,) = sqlx::query_as(&sql).fetch_one(pool).await.ok()?;
    Some(n)
}

async fn table_columns(pool: &crate::db::DbPool, table: &str) -> Option<Vec<ColumnInfo>> {
    #[cfg(feature = "sqlite")]
    {
        use sqlx::Row;
        let sql = format!("PRAGMA table_info(\"{table}\")");
        let rows = sqlx::query(&sql).fetch_all(pool).await.ok()?;
        Some(
            rows.iter()
                .map(|r| ColumnInfo {
                    name: r.try_get::<String, _>("name").unwrap_or_default(),
                    ty: r.try_get::<String, _>("type").unwrap_or_default(),
                })
                .collect(),
        )
    }
    #[cfg(not(feature = "sqlite"))]
    {
        #[cfg(feature = "postgres")]
        let sql = "SELECT column_name, data_type FROM information_schema.columns \
                   WHERE table_name = $1 ORDER BY ordinal_position";
        #[cfg(not(feature = "postgres"))]
        let sql = "SELECT COLUMN_NAME, COLUMN_TYPE FROM information_schema.COLUMNS \
                   WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = ? ORDER BY ORDINAL_POSITION";
        let rows: Vec<(String, String)> =
            sqlx::query_as(sql).bind(table).fetch_all(pool).await.ok()?;
        Some(
            rows.into_iter()
                .map(|(name, ty)| ColumnInfo { name, ty })
                .collect(),
        )
    }
}

/// List the application's tables (best-effort, driver-specific).
async fn list_tables(pool: &crate::db::DbPool) -> Option<Vec<String>> {
    #[cfg(feature = "mysql")]
    let sql = "SHOW TABLES";
    #[cfg(feature = "postgres")]
    let sql = "SELECT tablename FROM pg_tables WHERE schemaname = 'public' ORDER BY tablename";
    #[cfg(feature = "sqlite")]
    let sql = "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name";

    let rows: Vec<(String,)> = sqlx::query_as(sql).fetch_all(pool).await.ok()?;
    Some(
        rows.into_iter()
            .map(|(t,)| t)
            .filter(|t| !t.starts_with("_karbon_"))
            .collect(),
    )
}

#[derive(Deserialize)]
pub struct TerminalReq {
    pub command: String,
}

/// Sub-commands the Studio terminal is allowed to run. Deliberately excludes
/// `dev`/`build`/`serve` (long-running / recursive) and anything outside the CLI.
const TERMINAL_ALLOWED: &[&str] = &["generate", "g", "migrate", "doctor", "docs"];

/// JSON API: run a whitelisted `karbon` sub-command and return its output.
/// Dev-only, token-protected, no shell (args are passed literally). The binary is
/// resolved from `KARBON_BIN` (set by `karbon dev`) and falls back to `karbon` on PATH.
pub async fn api_terminal(
    State(state): State<StudioState>,
    Query(query): Query<TokenQuery>,
    headers: HeaderMap,
    axum::Json(req): axum::Json<TerminalReq>,
) -> Response {
    if !check_token(&state, &query, &headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if !studio_dev_open() {
        return (
            StatusCode::FORBIDDEN,
            "Terminal disabled outside development",
        )
            .into_response();
    }

    let parts: Vec<String> = req.command.split_whitespace().map(String::from).collect();
    let Some(first) = parts.first() else {
        return (StatusCode::BAD_REQUEST, "Empty command").into_response();
    };
    if !TERMINAL_ALLOWED.contains(&first.as_str()) {
        return axum::Json(serde_json::json!({
            "command": req.command,
            "code": serde_json::Value::Null,
            "stdout": "",
            "stderr": format!("Command `{first}` not allowed. Allowed: {}", TERMINAL_ALLOWED.join(", ")),
        }))
        .into_response();
    }
    if parts.len() > 16 {
        return (StatusCode::BAD_REQUEST, "Too many arguments").into_response();
    }

    let bin = std::env::var("KARBON_BIN").unwrap_or_else(|_| "karbon".to_string());
    // No stdin: the command is non-interactive here. Otherwise a TTY inherited
    // through `karbon dev → backend → karbon` would trigger interactive prompts
    // (e.g. the field assistant) that block forever.
    match tokio::process::Command::new(&bin)
        .args(&parts)
        .stdin(std::process::Stdio::null())
        .output()
        .await
    {
        Ok(out) => axum::Json(serde_json::json!({
            "command": req.command,
            "code": out.status.code(),
            "stdout": String::from_utf8_lossy(&out.stdout),
            "stderr": String::from_utf8_lossy(&out.stderr),
        }))
        .into_response(),
        Err(e) => axum::Json(serde_json::json!({
            "command": req.command,
            "code": serde_json::Value::Null,
            "stdout": "",
            "stderr": format!("Failed to run `{bin}`: {e} (is the `karbon` CLI on PATH?)"),
        }))
        .into_response(),
    }
}

const FRAMEWORK_REFERENCE: &str = include_str!("docs/reference.md");

/// JSON API: docs — the bundled Karbon reference + the project's own `docs/*.md`,
/// each rendered to HTML. Same Markdown sources as `karbon docs build`.
pub async fn api_docs(
    State(state): State<StudioState>,
    Query(query): Query<TokenQuery>,
    headers: HeaderMap,
) -> Response {
    if !check_token(&state, &query, &headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    let mut docs = vec![serde_json::json!({
        "slug": "karbon-reference",
        "title": "Karbon reference",
        "source": "framework",
        "html": render_markdown(FRAMEWORK_REFERENCE),
    })];

    // Project docs (CWD is the project root in dev).
    if let Ok(entries) = std::fs::read_dir("docs") {
        let mut paths: Vec<std::path::PathBuf> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "md"))
            .collect();
        paths.sort();
        for path in paths {
            let Ok(md) = std::fs::read_to_string(&path) else {
                continue;
            };
            let stem = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            let title = md
                .lines()
                .find_map(|l| l.trim().strip_prefix("# ").map(|h| h.trim().to_string()))
                .unwrap_or_else(|| stem.clone());
            docs.push(serde_json::json!({
                "slug": stem,
                "title": title,
                "source": "project",
                "html": render_markdown(&md),
            }));
        }
    }

    axum::Json(serde_json::json!({ "docs": docs })).into_response()
}

fn render_markdown(md: &str) -> String {
    use pulldown_cmark::{Options, Parser, html};
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    let parser = Parser::new_ext(md, opts);
    let mut out = String::new();
    html::push_html(&mut out, parser);
    out
}

/// JSON API: clear all data
pub async fn api_clear(
    State(state): State<StudioState>,
    Query(query): Query<TokenQuery>,
    headers: HeaderMap,
) -> Response {
    if !check_token(&state, &query, &headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    state.collector.clear().await;
    axum::Json(serde_json::json!({"ok": true})).into_response()
}

/// WebSocket endpoint for real-time updates
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<StudioState>,
    Query(query): Query<TokenQuery>,
    headers: HeaderMap,
) -> Response {
    if !check_token(&state, &query, &headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    ws.on_upgrade(move |socket| handle_ws(socket, state))
}

async fn handle_ws(mut socket: WebSocket, state: StudioState) {
    let mut rx = state.collector.subscribe();

    if let Ok(stats) = serde_json::to_string(&super::collector::StudioMessage::Stats(
        state.collector.get_stats().await,
    )) {
        let _ = socket.send(Message::Text(stats.into())).await;
    }

    loop {
        tokio::select! {
            msg = rx.recv() => {
                match msg {
                    Ok(json) => {
                        if socket.send(Message::Text(json.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
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
