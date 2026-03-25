from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]
DEPLOY_WORKFLOW = ROOT / ".github" / "workflows" / "deploy.yml"


def test_deploy_workflow_targets_split_railway_services():
    text = DEPLOY_WORKFLOW.read_text(encoding="utf-8")

    assert "--service heiwa-hub" in text
    assert "--service heiwa-trading" in text


def test_deploy_workflow_keeps_trading_deploy_internal():
    text = DEPLOY_WORKFLOW.read_text(encoding="utf-8")

    assert "https://api.heiwa.ltd/health" in text
    assert "ENABLE_TRADING_DEPLOY" in text
    assert "https://trade.heiwa.ltd/health" not in text
