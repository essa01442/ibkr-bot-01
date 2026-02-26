use log::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    info!("Starting Robust Penny Scalper v7.0 FINAL");

    let config_path = "configs/default.toml";
    let config_str = std::fs::read_to_string(config_path)
        .map_err(|e| format!("Failed to read {}: {}", config_path, e))?;
    let config: core_types::config::AppConfig =
        toml::from_str(&config_str).map_err(|e| format!("Failed to parse config: {}", e))?;

    app_runtime::run(config).await?;
    Ok(())
}
