from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]
DEPLOY_WORKFLOW = ROOT / ".github" / "workflows" / "deploy.yml"


def test_deploy_workflow_targets_split_railway_services():
    text = DEPLOY_WORKFLOW.read_text(encoding="utf-8")

    assert "--service heiwa-cloud-hq" in text
    assert "--service heiwa-trading" in text


def test_deploy_workflow_verifies_trade_runtime_endpoint():
    text = DEPLOY_WORKFLOW.read_text(encoding="utf-8")

    assert "https://api.heiwa.ltd/health" in text
    assert "https://trade.heiwa.ltd/health" in text
