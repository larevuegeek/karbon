use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use serde::Deserialize;

use super::collector::StudioCollector;

#[derive(Clone)]
pub struct StudioState {
    pub collector: StudioCollector,
    pub token: String,
}

#[derive(Deserialize)]
pub struct TokenQuery {
    pub token: Option<String>,
}

fn check_token(state: &StudioState, query: &TokenQuery, headers: &HeaderMap) -> bool {
    // 1. Check query param (initial access)
    if let Some(t) = &query.token {
        if t == &state.token {
            return true;
        }
    }
    // 2. Check cookie (subsequent requests — avoids token in URLs/logs)
    if let Some(cookie) = headers.get("cookie") {
        if let Ok(cookie_str) = cookie.to_str() {
            for part in cookie_str.split(';') {
                let part = part.trim();
                if let Some(val) = part.strip_prefix("_studio_token=") {
                    if val == state.token {
                        return true;
                    }
                }
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
    let cookie = format!(
        "_studio_token={}; Path=/_studio; HttpOnly; SameSite=Strict",
        state.token
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
