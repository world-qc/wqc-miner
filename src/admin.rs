use std::sync::Arc;
use std::time::Duration;

use axum::{
    extract::{
        ws::{Message, WebSocket},
        Query, State, WebSocketUpgrade,
    },
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

use crate::config::{SettingsUpdate, SettingsView};
use crate::data_dir::DataLayout;
use crate::logs::{self, LogSource};
use crate::supervisor::{StatusIssue, Supervisor};

#[derive(Clone)]
pub struct AdminState {
    pub layout: DataLayout,
    pub settings: Arc<RwLock<crate::config::MinerSettings>>,
    pub supervisor: Arc<RwLock<Supervisor>>,
}

pub fn router(state: AdminState) -> Router {
    let static_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("static");

    Router::new()
        .route("/", get(index))
        .route("/api/status", get(api_status))
        .route("/api/settings", get(api_get_settings).put(api_put_settings))
        .route("/api/mining/start", post(api_start))
        .route("/api/mining/stop", post(api_stop))
        .route("/api/node-status", get(api_node_status))
        .route("/api/logs/ws", get(api_logs_ws))
        .nest_service("/static", ServeDir::new(static_dir))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

async fn index() -> Html<&'static str> {
    Html(include_str!("../static/index.html"))
}

async fn api_status(State(state): State<AdminState>) -> Json<serde_json::Value> {
    let mut supervisor = state.supervisor.write().await;
    let settings = state.settings.read().await;
    let mining = supervisor.status().await;
    let core_tn = supervisor.core_tn_status().await;
    Json(serde_json::json!({
        "mining": mining,
        "settings": SettingsView::from_settings(&settings, &state.layout),
        "core_tn": core_tn,
    }))
}

async fn api_get_settings(State(state): State<AdminState>) -> Json<SettingsView> {
    let settings = state.settings.read().await;
    Json(SettingsView::from_settings(&settings, &state.layout))
}

#[derive(Debug, Deserialize)]
struct PutSettingsBody {
    #[serde(flatten)]
    update: SettingsUpdate,
}

async fn api_put_settings(
    State(state): State<AdminState>,
    Json(body): Json<PutSettingsBody>,
) -> Result<Json<SettingsView>, ApiError> {
    let mut settings = state.settings.write().await;
    body.update.apply(&mut settings);
    settings
        .save(&state.layout)
        .map_err(|e| ApiError::internal(e.to_string()))?;

    let mut supervisor = state.supervisor.write().await;
    supervisor.reload_settings(settings.clone());

    Ok(Json(SettingsView::from_settings(&settings, &state.layout)))
}

async fn api_start(State(state): State<AdminState>) -> Result<Json<serde_json::Value>, ApiError> {
    let mut supervisor = state.supervisor.write().await;
    let settings = state.settings.read().await;
    supervisor.reload_settings(settings.clone());
    drop(settings);

    match supervisor.start().await {
        Ok(status) => Ok(Json(serde_json::json!({ "ok": true, "status": status }))),
        Err(issue) => Err(ApiError::from_issue(issue)),
    }
}

async fn api_stop(State(state): State<AdminState>) -> Result<Json<serde_json::Value>, ApiError> {
    let mut supervisor = state.supervisor.write().await;
    let status = supervisor
        .stop()
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Json(serde_json::json!({ "ok": true, "status": status })))
}

async fn api_node_status(
    State(state): State<AdminState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let supervisor = state.supervisor.read().await;
    let body = supervisor
        .node_status_json()
        .await
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    Ok(Json(body))
}

#[derive(Debug, Deserialize)]
struct LogsWsQuery {
    source: Option<String>,
    lines: Option<usize>,
}

async fn api_logs_ws(
    ws: WebSocketUpgrade,
    Query(query): Query<LogsWsQuery>,
    State(state): State<AdminState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_logs_socket(socket, state, query))
}

async fn handle_logs_socket(socket: WebSocket, state: AdminState, query: LogsWsQuery) {
    let source = match LogSource::parse(query.source.as_deref().unwrap_or("core")) {
        Some(s) => s,
        None => {
            let (mut sink, _) = socket.split();
            let _ = sink
                .send(Message::Text(
                    serde_json::json!({
                        "type": "error",
                        "message": "source must be 'core' or 'node'"
                    })
                    .to_string(),
                ))
                .await;
            return;
        }
    };

    let max_lines = logs::clamp_lines(query.lines);
    let path = logs::path_for(source, &state.layout);
    let (mut sink, mut stream) = socket.split();

    let tail = match logs::read_tail(&path, max_lines) {
        Ok(t) => t,
        Err(err) => {
            let _ = sink
                .send(Message::Text(
                    serde_json::json!({
                        "type": "error",
                        "message": format!("failed to read log: {err}")
                    })
                    .to_string(),
                ))
                .await;
            return;
        }
    };

    let hello = serde_json::json!({
        "type": "hello",
        "source": source.as_str(),
        "path": tail.path,
        "lines": tail.lines,
        "truncated": tail.truncated,
        "exists": tail.exists,
        "mtime_unix": tail.mtime_unix,
    });
    if sink.send(Message::Text(hello.to_string())).await.is_err() {
        return;
    }

    let mut offset = tail.follow_offset;
    let mut pending = String::new();
    let mut ticker = tokio::time::interval(Duration::from_millis(400));

    loop {
        tokio::select! {
            msg = stream.next() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Ping(p))) => {
                        if sink.send(Message::Pong(p)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(_)) => {}
                    Some(Err(_)) => break,
                }
            }
            _ = ticker.tick() => {
                match logs::read_new_lines(&path, offset, &mut pending) {
                    Ok((lines, new_offset)) => {
                        offset = new_offset;
                        for line in lines {
                            let payload = serde_json::json!({
                                "type": "line",
                                "source": source.as_str(),
                                "line": line,
                            });
                            if sink.send(Message::Text(payload.to_string())).await.is_err() {
                                return;
                            }
                        }
                    }
                    Err(err) => {
                        let payload = serde_json::json!({
                            "type": "error",
                            "message": format!("log follow error: {err}")
                        });
                        let _ = sink.send(Message::Text(payload.to_string())).await;
                        break;
                    }
                }
            }
        }
    }
}

struct ApiError {
    status: StatusCode,
    message: String,
    code: Option<String>,
}

impl ApiError {
    fn bad_request(message: String) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message,
            code: None,
        }
    }

    fn internal(message: String) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message,
            code: None,
        }
    }

    fn from_issue(issue: StatusIssue) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: issue.message,
            code: Some(issue.code),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut body = serde_json::json!({ "error": self.message });
        if let Some(code) = self.code {
            body["code"] = serde_json::json!(code);
        }
        (self.status, Json(body)).into_response()
    }
}
