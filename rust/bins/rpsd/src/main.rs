use log::info;
use std::sync::Arc;
use tokio::sync::broadcast;

mod dashboard;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    info!("Starting Robust Penny Scalper v7.0 FINAL");

    let config_path = "configs/default.toml";
    let config_str = std::fs::read_to_string(config_path)
        .map_err(|e| format!("Failed to read {}: {}", config_path, e))?;
    let config: core_types::config::AppConfig =
        toml::from_str(&config_str).map_err(|e| format!("Failed to parse config: {}", e))?;

    // Create broadcast channel for WebSocket server
    let (tx, _rx) = broadcast::channel(100);
    let dashboard_state = Arc::new(dashboard::DashboardState { tx: tx.clone() });

    // Spawn task to broadcast dummy SystemSnapshot every 250ms
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(250));
        loop {
            interval.tick().await;
            let snapshot = dashboard::SystemSnapshot {
                ts_ms: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64,
                regime: "Normal".to_string(),
                data_quality: "Healthy".to_string(),
                monitor_only: false,
                session_state: "Open".to_string(),
                daily_pnl_usd: 0.0,
                loss_ladder_level: 0,
                max_daily_loss_remaining: 100.0,
                open_positions: 0,
                oms_state: "Active".to_string(),
                p95_latency_us: 1000,
                ibkr_subscription_count: 0,
                ibkr_subscription_budget: 100,
                recent_rejects: vec![],
                recent_alerts: vec![],
            };
            let _ = tx.send(snapshot);
        }
    });

    let app = dashboard::router(dashboard_state);
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    info!("Starting Dashboard Server on 0.0.0.0:8080");

    // Spawn the dashboard server and the main application runtime
    tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            log::error!("Dashboard Server error: {}", e);
        }
    });

    app_runtime::run(config).await?;

    Ok(())
}
