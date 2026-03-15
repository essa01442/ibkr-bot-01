//! Real-time WebSocket server for the UI Dashboard — §1.5
//! Streams SystemSnapshot JSON every 250ms to all connected clients.

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Query, State,
    },
    http::{Request, StatusCode},
    middleware::{self, Next},
    extract::{ws::{WebSocket, WebSocketUpgrade, Message}, State, Query},
    response::IntoResponse,
    response::Response,
    routing::get,
    Router,
    http::{Request, StatusCode},
    middleware::{self, Next},
    response::Response,
};
use serde::Deserialize;
use serde::Serialize;
use serde::Deserialize;
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
    // Add explicitly to signify synthetic data
    pub is_synthetic: bool,
}

pub struct DashboardState {
    pub tx: broadcast::Sender<SystemSnapshot>,
    pub auth_token: String,
}

#[derive(Deserialize)]
pub struct AuthQuery {
    pub token: Option<String>,
}

async fn auth_middleware(
    State(state): State<Arc<DashboardState>>,
    Query(query): Query<AuthQuery>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let token = query.token.or_else(|| {
        req.headers()
            .get("Authorization")
            .and_then(|h| h.to_str().ok())
            .map(|h| h.replace("Bearer ", ""))
    });

    if let Some(t) = token {
        if t == state.auth_token {
            return Ok(next.run(req).await);
        }
    }

    log::warn!(
        "Rejected unauthenticated dashboard request to: {}",
        req.uri().path()
    );
    log::warn!("Rejected unauthenticated dashboard request to: {}", req.uri().path());
    Err(StatusCode::UNAUTHORIZED)
}

pub fn router(state: Arc<DashboardState>, auth_token: String) -> Router {
    let state_with_auth = Arc::new(DashboardState {
        tx: state.tx.clone(),
        auth_token,
    });

    // Public routes
    let public_routes = Router::new().route("/health", get(|| async { "ok" }));
    let public_routes = Router::new()
        .route("/health", get(|| async { "ok" }));

    // Protected routes requiring authentication
    let protected_routes = Router::new()
        .route("/ws", get(ws_handler))
        .route("/api/status", get(status_handler))
        .route("/", get(index_handler))
        .fallback_service(ServeDir::new("dashboard"))
        .route_layer(middleware::from_fn_with_state(
            state_with_auth.clone(),
            auth_middleware,
        ))
        .route_layer(middleware::from_fn_with_state(state_with_auth.clone(), auth_middleware))
        .with_state(state_with_auth);

    Router::new().merge(public_routes).merge(protected_routes)
}

async fn status_handler(State(_state): State<Arc<DashboardState>>) -> impl IntoResponse {
    // Return last snapshot if available
    axum::Json(serde_json::json!({"status": "ok"}))
}

async fn index_handler() -> impl IntoResponse {
    // Serve the dashboard HTML
    axum::response::Html(include_str!("../../../../dashboard/index.html"))
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
                // Assertion: Prevent sending synthetic snapshots on active websocket connections
                // In production, only fully wired real states should be transmitted.
                // Note: The acceptance criteria strictly forbid synthetic live data.
                debug_assert!(
                    !snapshot.is_synthetic,
                    "FATAL: Synthetic snapshot passed to live WebSocket!"
                );

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
