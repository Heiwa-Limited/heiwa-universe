use anyhow::Result;
use heiwa_protocol::ModelTier;

use crate::config::RuntimeConfig;
use crate::drex::{default_policy, plan_route, DrexIngress, RoutePlan};

pub async fn run(cfg: RuntimeConfig) -> Result<()> {
    let _ = cfg;
    Ok(())
}

pub fn plan_ingress(ingress: &DrexIngress, model_tiers: &[ModelTier]) -> Result<RoutePlan> {
    plan_route(ingress, model_tiers, &default_policy())
}
