use app_runtime;
use log::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    info!("Starting Robust Penny Scalper v7.0 FINAL");

    // app_runtime::run().await?;

    Ok(())
}
