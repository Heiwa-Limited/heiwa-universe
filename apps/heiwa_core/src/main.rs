#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cfg = heiwa_core::config::RuntimeConfig::from_env();
    heiwa_core::runtime::run(cfg).await
}
