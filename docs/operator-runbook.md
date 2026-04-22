# Operator Runbook

## Boot sequence

Read these before runtime changes:

1. `SOUL.md`
2. `AGENTS.md`
3. `config/swarm/BUILD_BLUEPRINT_2026-03-06.md`
4. `config/swarm/ai_router.json`
5. `config/identities/profiles.json`
6. `docs/railway-self-operation.md`

## Basic checks

```bash
source .venv/bin/activate
python apps/heiwa_hub/tests/test_intent_classifier.py
python apps/heiwa_hub/tests/test_risk_scorer.py
python apps/heiwa_hub/tests/test_compute_router.py
python -m pip install -r docs/requirements.txt
mkdocs build --strict
python apps/heiwa_app/scripts/check_static_surface.py
```

## Public surface rule

If a surface is not verified by tests or build checks, it should not be described as stack-complete in docs, README, or the static web shell.
