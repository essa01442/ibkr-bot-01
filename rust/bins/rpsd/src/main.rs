#![deny(clippy::unwrap_in_result)]
use log::info;
use std::sync::Arc;
use tokio::sync::broadcast;

mod dashboard;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Catch all panics — log them before crash, enable post-mortem analysis
    std::panic::set_hook(Box::new(|info| {
        let location = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_else(|| "unknown".to_string());
        let payload = info
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| info.payload().downcast_ref::<String>().map(|s| s.as_str()))
            .unwrap_or("non-string panic payload");
        log::error!("PANIC at {}: {}", location, payload);
        // Give logger time to flush
        std::thread::sleep(std::time::Duration::from_millis(100));
    }));

    env_logger::init();
    info!("Starting Robust Penny Scalper v7.0");

    let config_path = "configs/default.toml";
    let config_str = std::fs::read_to_string(config_path)
        .map_err(|e| format!("Failed to read {}: {}", config_path, e))?;
    let config: core_types::config::AppConfig =
        toml::from_str(&config_str).map_err(|e| format!("Failed to parse config: {}", e))?;

    // Create broadcast channel for WebSocket server
    let (tx, _rx) = broadcast::channel(100);
    let dashboard_state = Arc::new(dashboard::DashboardState {
        tx: tx.clone(),
        auth_token: config.dashboard.auth_token.clone(),
    });

    // Ensure we do not broadcast synthetic 'Normal' / 0.0 PNL as live data
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(1000));
        loop {
            interval.tick().await;
            let snapshot = dashboard::SystemSnapshot {
                ts_ms: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64,
                regime: "NOT_WIRED".to_string(),
                data_quality: "NOT_WIRED".to_string(),
                monitor_only: true, // fail-safe
                session_state: "NOT_WIRED".to_string(),
                daily_pnl_usd: f64::NAN, // Clearly invalid/not 0.0
                loss_ladder_level: 0,
                max_daily_loss_remaining: f64::NAN,
                open_positions: 0,
                oms_state: "NOT_WIRED".to_string(),
                p95_latency_us: 0,
                ibkr_subscription_count: 0,
                ibkr_subscription_budget: 0,
                recent_rejects: vec!["SYSTEM_NOT_WIRED".to_string()],
                recent_alerts: vec!["DASHBOARD_NOT_WIRED".to_string()],
                is_synthetic: true, // Marker for assertion
            };
            let _ = tx.send(snapshot);
        }
    });

    let bind_addr = &config.dashboard.bind_address;
    let is_localhost = bind_addr.starts_with("127.0.0.1") || bind_addr.starts_with("localhost");

    if !is_localhost {
        log::warn!(
            "SECURITY WARNING: Dashboard is configured to bind to a non-localhost address ({})",
            bind_addr
        );
        if !config.dashboard.allow_insecure_remote {
            log::error!("FATAL: ws:// on non-localhost is disabled by default for security. Set dashboard.allow_insecure_remote = true to override.");
            std::process::exit(1);
        }
    }

    if config.dashboard.auth_token.trim().is_empty() {
        log::error!("FATAL: dashboard.auth_token is required but missing or empty in configuration. Refusing to start.");
        std::process::exit(1);
    }

    let app = dashboard::router(dashboard_state, config.dashboard.auth_token.clone());
    let listener = tokio::net::TcpListener::bind(bind_addr).await.unwrap();
    info!("Starting Dashboard Server on {}", bind_addr);

    // Run both the dashboard server and the main application runtime
    tokio::select! {
        res = axum::serve(listener, app) => {
            if let Err(e) = res {
                log::error!("Dashboard Server error: {}", e);
            }
        }
        res = app_runtime::run(config) => {
            res?;
        }
    }

    Ok(())
}
