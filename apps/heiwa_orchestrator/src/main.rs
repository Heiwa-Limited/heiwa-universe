#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cfg = heiwa_orchestrator::config::RuntimeConfig::from_env();
    heiwa_orchestrator::runtime::run(cfg).await
}
