//! Real-time WebSocket server for the UI Dashboard — §1.5
//! Streams SystemSnapshot JSON every 250ms to all connected clients.

use axum::{
    extract::{ws::{WebSocket, WebSocketUpgrade, Message}, State},
    response::IntoResponse,
    routing::get,
    Router,
};
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::broadcast;
use tower_http::services::ServeDir;

/// Snapshot of system state — streamed to UI every 250ms.
#[derive(Debug, Clone, Serialize)]
pub struct SystemSnapshot {
    pub ts_ms: u64,
    pub regime: String,
    pub data_quality: String,
    pub monitor_only: bool,
    pub session_state: String,
    pub daily_pnl_usd: f64,
    pub loss_ladder_level: u32,
    pub max_daily_loss_remaining: f64,
    pub open_positions: usize,
    pub oms_state: String,
    pub p95_latency_us: u64,
    pub ibkr_subscription_count: u32,
    pub ibkr_subscription_budget: u32,
    // Recent reject reasons (last 5)
    pub recent_rejects: Vec<String>,
    // Recent alerts (last 3)
    pub recent_alerts: Vec<String>,
}

pub struct DashboardState {
    pub tx: broadcast::Sender<SystemSnapshot>,
}

pub fn router(state: Arc<DashboardState>) -> Router {
    Router::new()
        .fallback_service(ServeDir::new("dashboard"))
        .route("/ws", get(ws_handler))
        .route("/health", get(|| async { "ok" }))
        .with_state(state)
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<DashboardState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: Arc<DashboardState>) {
    let mut rx = state.tx.subscribe();
    loop {
        match rx.recv().await {
            Ok(snapshot) => {
                let json = serde_json::to_string(&snapshot).unwrap_or_default();
                if socket.send(Message::Text(json)).await.is_err() {
                    break; // Client disconnected
                }
            }
            Err(broadcast::error::RecvError::Closed) => break,
            Err(broadcast::error::RecvError::Lagged(_)) => {} // Skip missed frames
        }
    }
}
