use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cfg = heiwa_core::config::RuntimeConfig::from_env();

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| format!("{},tower_http=debug", cfg.log_level).into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    heiwa_core::runtime::run(cfg).await
}
