use anyhow::Result;

use crate::config::RuntimeConfig;

pub async fn run(cfg: RuntimeConfig) -> Result<()> {
    let _ = cfg;
    Ok(())
}
