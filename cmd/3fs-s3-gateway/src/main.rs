use threefs_gateway::{config::GatewayConfig, run_server};
use tracing_subscriber::{fmt, EnvFilter};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    fmt().with_env_filter(filter).json().init();
    let cfg = GatewayConfig::from_env()?;
    run_server(cfg).await
}

