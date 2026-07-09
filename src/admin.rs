use std::sync::Arc;

use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

use crate::config::{SettingsUpdate, SettingsView};
use crate::data_dir::DataLayout;
use crate::supervisor::Supervisor;

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
        .nest_service("/static", ServeDir::new(static_dir))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

async fn index() -> Html<&'static str> {
    Html(include_str!("../static/index.html"))
}

async fn api_status(State(state): State<AdminState>) -> Json<serde_json::Value> {
    let supervisor = state.supervisor.read().await;
    let settings = state.settings.read().await;
    let mining = supervisor.status();
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

    let status = supervisor
        .start()
        .await
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    Ok(Json(serde_json::json!({ "ok": true, "status": status })))
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

struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn bad_request(message: String) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message,
        }
    }

    fn internal(message: String) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(serde_json::json!({ "error": self.message })),
        )
            .into_response()
    }
}
