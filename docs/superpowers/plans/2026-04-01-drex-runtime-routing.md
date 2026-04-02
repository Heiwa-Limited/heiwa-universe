# DREX Runtime Routing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the ADE paper load-bearing by introducing typed DREX runtime objects, config-backed tier scoring, and persisted DREX decision/failure records in Heiwa's active routing path.

**Architecture:** This slice does not replace the whole router. It adds a shared DREX protocol model, a cognition-side scorer with static-but-versioned weights, and a narrow integration path where DREX selects `macro` / `meso` / `micro` resolution and the existing router maps that choice onto current execution lanes. Authority leaves the DREX vector and stays a policy gate; observability becomes a score modifier; DREX decisions and failures are persisted to STDB so later calibration can be data-driven instead of hand-waved. Rust-native DREX scoring, TypeScript/web exposure, and Cloudflare-facing projection surfaces are explicitly deferred so this first slice stays bounded.

**Tech Stack:** Python 3.11 dataclasses, Heiwa protocol package, Heiwa cognition router, Rust SpacetimeDB module, STDB CLI bridge, JSON config, pytest

**Spec:** `docs/enterprise/HEIWA_AGENTIC_DIGITAL_ENTITY_DREX_2026-04-01.md`

---

## Constraints

- Keep the first DREX slice bounded. Do not implement the full memory-policy engine here.
- Collapse overlapping DREX axes. `authority` is a policy gate, not a scored axis. `observability` is a score modifier, not a primary axis.
- Use static policy weights for the first rollout, but make the policy versioned and telemetry-backed so calibration can be added without redesigning the type system.
- Preserve current privacy/runtime guardrails. DREX may refine lane selection, but it must not violate sovereign clamps or Railway/local runtime constraints.
- Avoid making STDB mandatory for the scorer. When STDB is absent, routing still works; only decision/failure persistence is skipped.
- The current STDB module uses `String` and `u64` time fields today. Do not turn this plan into a repo-wide timestamp-type migration. Use sortable numeric fields for new DREX rows where query ordering matters and document native timestamp migration as follow-on work.
- `route_decisions` already exists. DREX persistence must link to it explicitly instead of creating an unrelated second audit trail.

## Out of Scope

- The `M_policy(DREX, state, trace_value, storage_budget)` memory-policy engine
- Learned weight updates or online gradient tuning
- Rewriting every intent/risk heuristic in `ComputeRouter`
- UI/dashboard work for DREX visualization
- Rust-native DREX scoring and projection logic
- Cloudflare/public projection surfaces for DREX-aware status views
- Repo-wide migration of STDB time fields to native timestamp types

## File Map

### New files

| File | Responsibility |
|------|---------------|
| `packages/heiwa_protocol/heiwa_protocol/drex.py` | Shared DREX protocol objects, enums, serialization helpers |
| `packages/heiwa_cognition/heiwa_cognition/drex.py` | DREX vector construction, score calculation, tie handling, and failure classification |
| `config/swarm/drex_policy.json` | Versioned default DREX axes, weights, modifiers, thresholds, and calibration mode |
| `apps/heiwa_hub/tests/test_drex_protocol.py` | Protocol-level tests for DREX dataclasses and serialization |
| `apps/heiwa_hub/tests/test_drex_scoring.py` | Unit tests for DREX vector construction, modifiers, and tier scoring |
| `apps/heiwa_hub/tests/test_drex_failure_taxonomy.py` | Unit tests for DREX failure modes, router fallback behavior, and STDB persistence calls |

### Modified files

| File | Change |
|------|--------|
| `packages/heiwa_protocol/heiwa_protocol/__init__.py` | Export DREX protocol objects |
| `packages/heiwa_protocol/heiwa_protocol/routing.py` | Add DREX payload fields to `BrokerRouteResult` |
| `packages/heiwa_cognition/heiwa_cognition/__init__.py` | Export DREX scorer APIs |
| `packages/heiwa_cognition/heiwa_cognition/router.py` | Attach DREX to `ComputeRoute` / `RoutedPlan`, run score evaluation, enforce DREX-tier application, and record DREX telemetry/failures |
| `apps/heiwa_hub/cognition/__init__.py` | Re-export DREX objects through the hub compatibility surface |
| `apps/heiwa_hub/spacetimedb/src/lib.rs` | Add `drex_decisions` and `drex_failures` tables plus reducers |
| `packages/heiwa_sdk/heiwa_sdk/spacetimedb.py` | Add DREX decision/failure reducer helpers and query helpers |
| `apps/heiwa_hub/tests/test_compute_router.py` | Assert DREX tiering does not break existing deterministic route expectations |
| `apps/heiwa_hub/tests/test_compute_router_stdb.py` | Assert DREX fields are attached when STDB-backed tiers are used |
| `apps/heiwa_hub/tests/test_phase3_integration.py` | Assert DREX decisions persist and failure records are emitted on fallback/escalation paths |

### Deferred follow-on spec

| File | Why it is deferred |
|------|--------------------|
| `docs/superpowers/specs/2026-04-01-drex-memory-policy-design.md` | Needed for `retain/fold/compact/project/discard`, but not required to make routing load-bearing |

### Deferred follow-on infrastructure work

| Area | Why it is deferred |
|------|--------------------|
| Rust-native scorer in `apps/heiwa_hub/spacetimedb/src/lib.rs` or a Rust routing layer | The first slice keeps scoring in Python to attach to the live router quickly; moving invariant-preserving DREX math into Rust is a second slice |
| TypeScript/web exposure in `apps/heiwa_web/` | Current web surface is not yet the DREX inspection surface; typed dashboard objects can follow once routing decisions are stable |
| Railway policy bundling and deployment docs | The first slice uses env/config fallback for policy loading; packaging the policy into deploy images is follow-on hardening |
| Cloudflare/public reduction surfaces | Public edge projection should consume already-reduced state, not raw routing internals; not part of v1 |

---

### Task 1: Add shared DREX protocol objects

**Files:**
- Create: `packages/heiwa_protocol/heiwa_protocol/drex.py`
- Modify: `packages/heiwa_protocol/heiwa_protocol/__init__.py`
- Test: `apps/heiwa_hub/tests/test_drex_protocol.py`

- [ ] **Step 1: Write the failing protocol test**

`apps/heiwa_hub/tests/test_drex_protocol.py`:

```python
from heiwa_protocol.drex import (
    DrexVector,
    DrexModifiers,
    DrexAuthorityGate,
    DrexScoreCard,
    DrexDecision,
    DrexFailureMode,
    DrexFailureRecord,
)


def test_drex_vector_has_seven_axes_only():
    vector = DrexVector(
        scope=0.5,
        abstraction=0.8,
        context_span=0.7,
        execution_proximity=0.2,
        blast_radius=0.6,
        coordination_load=0.4,
        latency_pressure=0.3,
    )
    payload = vector.to_dict()
    assert sorted(payload.keys()) == [
        "abstraction",
        "blast_radius",
        "context_span",
        "coordination_load",
        "execution_proximity",
        "latency_pressure",
        "scope",
    ]


def test_drex_decision_round_trips():
    decision = DrexDecision(
        vector=DrexVector(0.4, 0.7, 0.6, 0.3, 0.8, 0.5, 0.2),
        modifiers=DrexModifiers(observability=0.9, runtime_fit=1.0, history_confidence=0.75),
        gate=DrexAuthorityGate(authority_required="approved_write", requires_approval=True, reasons=["blast_radius"]),
        scorecard=DrexScoreCard(macro=0.81, meso=0.62, micro=0.29, chosen_tier="macro", confidence=0.19, policy_version="2026-04-01"),
        reasons=["broad scope", "high blast radius"],
        failure_modes=[DrexFailureMode.NONE],
    )
    restored = DrexDecision.from_dict(decision.to_dict())
    assert restored.scorecard.chosen_tier == "macro"
    assert restored.gate.requires_approval is True
```

- [ ] **Step 2: Run the test and confirm it fails**

Run:

```bash
cd /Users/dmcgregsauce/heiwa
pytest apps/heiwa_hub/tests/test_drex_protocol.py -q
```

Expected: FAIL because `heiwa_protocol.drex` does not exist yet.

- [ ] **Step 3: Implement the DREX protocol dataclasses**

`packages/heiwa_protocol/heiwa_protocol/drex.py`:

```python
from __future__ import annotations

from dataclasses import asdict, dataclass, field
from enum import StrEnum


class DrexFailureMode(StrEnum):
    NONE = "none"
    LOW_CONFIDENCE_TIE = "low_confidence_tie"
    POLICY_GATE_OVERRIDE = "policy_gate_override"
    EXECUTION_MISMATCH = "execution_mismatch"
    POLICY_MISSING = "policy_missing"
    VECTOR_EXTRACTION_ERROR = "vector_extraction_error"
    FALLBACK_ESCALATION = "fallback_escalation"


@dataclass(slots=True)
class DrexVector:
    scope: float
    abstraction: float
    context_span: float
    execution_proximity: float
    blast_radius: float
    coordination_load: float
    latency_pressure: float

    def to_dict(self) -> dict[str, float]:
        return asdict(self)

    @classmethod
    def from_dict(cls, payload: dict[str, float]) -> "DrexVector":
        return cls(**payload)


@dataclass(slots=True)
class DrexModifiers:
    observability: float
    runtime_fit: float
    history_confidence: float

    def to_dict(self) -> dict[str, float]:
        return asdict(self)

    @classmethod
    def from_dict(cls, payload: dict[str, float]) -> "DrexModifiers":
        return cls(**payload)


@dataclass(slots=True)
class DrexAuthorityGate:
    authority_required: str
    requires_approval: bool
    reasons: list[str] = field(default_factory=list)

    def to_dict(self) -> dict[str, object]:
        return asdict(self)

    @classmethod
    def from_dict(cls, payload: dict[str, object]) -> "DrexAuthorityGate":
        return cls(**payload)


@dataclass(slots=True)
class DrexScoreCard:
    macro: float
    meso: float
    micro: float
    chosen_tier: str
    confidence: float
    policy_version: str

    def to_dict(self) -> dict[str, object]:
        return asdict(self)

    @classmethod
    def from_dict(cls, payload: dict[str, object]) -> "DrexScoreCard":
        return cls(**payload)


@dataclass(slots=True)
class DrexFailureRecord:
    failure_mode: DrexFailureMode
    stage: str
    details: dict[str, object] = field(default_factory=dict)
    recovered: bool = False

    def to_dict(self) -> dict[str, object]:
        payload = asdict(self)
        payload["failure_mode"] = str(self.failure_mode)
        return payload

    @classmethod
    def from_dict(cls, payload: dict[str, object]) -> "DrexFailureRecord":
        return cls(
            failure_mode=DrexFailureMode(str(payload["failure_mode"])),
            stage=str(payload["stage"]),
            details=dict(payload.get("details") or {}),
            recovered=bool(payload.get("recovered", False)),
        )


@dataclass(slots=True)
class DrexDecision:
    vector: DrexVector
    modifiers: DrexModifiers
    gate: DrexAuthorityGate
    scorecard: DrexScoreCard
    reasons: list[str] = field(default_factory=list)
    failure_modes: list[DrexFailureMode] = field(default_factory=list)
    failure_records: list[DrexFailureRecord] = field(default_factory=list)

    def to_dict(self) -> dict[str, object]:
        return {
            "vector": self.vector.to_dict(),
            "modifiers": self.modifiers.to_dict(),
            "gate": self.gate.to_dict(),
            "scorecard": self.scorecard.to_dict(),
            "reasons": list(self.reasons),
            "failure_modes": [str(item) for item in self.failure_modes],
            "failure_records": [item.to_dict() for item in self.failure_records],
        }

    @classmethod
    def from_dict(cls, payload: dict[str, object]) -> "DrexDecision":
        return cls(
            vector=DrexVector.from_dict(dict(payload["vector"])),
            modifiers=DrexModifiers.from_dict(dict(payload["modifiers"])),
            gate=DrexAuthorityGate.from_dict(dict(payload["gate"])),
            scorecard=DrexScoreCard.from_dict(dict(payload["scorecard"])),
            reasons=list(payload.get("reasons") or []),
            failure_modes=[DrexFailureMode(str(item)) for item in (payload.get("failure_modes") or [])],
            failure_records=[DrexFailureRecord.from_dict(dict(item)) for item in (payload.get("failure_records") or [])],
        )
```

- [ ] **Step 4: Export DREX from the protocol package**

Modify `packages/heiwa_protocol/heiwa_protocol/__init__.py`:

```python
from .drex import (
    DrexVector,
    DrexModifiers,
    DrexAuthorityGate,
    DrexScoreCard,
    DrexDecision,
    DrexFailureMode,
    DrexFailureRecord,
)
```

- [ ] **Step 5: Run the protocol test and make sure it passes**

Run:

```bash
cd /Users/dmcgregsauce/heiwa
pytest apps/heiwa_hub/tests/test_drex_protocol.py -q
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add packages/heiwa_protocol/heiwa_protocol/drex.py \
  packages/heiwa_protocol/heiwa_protocol/__init__.py \
  apps/heiwa_hub/tests/test_drex_protocol.py
git commit -m "feat: add shared drex protocol types"
```

---

### Task 2: Add config-backed DREX scoring and calibration hooks

**Files:**
- Create: `packages/heiwa_cognition/heiwa_cognition/drex.py`
- Create: `config/swarm/drex_policy.json`
- Modify: `packages/heiwa_cognition/heiwa_cognition/__init__.py`
- Test: `apps/heiwa_hub/tests/test_drex_scoring.py`

- [ ] **Step 1: Write the failing scorer test**

`apps/heiwa_hub/tests/test_drex_scoring.py`:

```python
from pathlib import Path

from heiwa_cognition.drex import evaluate_drex


def test_build_task_biases_micro_over_macro():
    decision = evaluate_drex(
        intent="build",
        risk="medium",
        raw_text="patch router.py and run tests",
        privacy="local",
        runtime="railway",
        policy_path=Path("/Users/dmcgregsauce/heiwa/config/swarm/drex_policy.json"),
    )
    assert decision.scorecard.chosen_tier == "micro"


def test_strategy_task_biases_macro():
    decision = evaluate_drex(
        intent="strategy",
        risk="high",
        raw_text="re-balance enterprise priorities across products",
        privacy="local",
        runtime="railway",
        policy_path=Path("/Users/dmcgregsauce/heiwa/config/swarm/drex_policy.json"),
    )
    assert decision.scorecard.chosen_tier == "macro"


def test_low_observability_penalizes_micro():
    clear = evaluate_drex("build", "low", "edit one file and run pytest", "local", "railway", observability=1.0)
    blind = evaluate_drex("build", "low", "edit one file and run pytest", "local", "railway", observability=0.1)
    assert blind.scorecard.micro < clear.scorecard.micro
```

- [ ] **Step 2: Run the scorer test and confirm it fails**

Run:

```bash
cd /Users/dmcgregsauce/heiwa
pytest apps/heiwa_hub/tests/test_drex_scoring.py -q
```

Expected: FAIL because the scorer module and policy file do not exist.

- [ ] **Step 3: Seed the first DREX policy file**

`config/swarm/drex_policy.json`:

```json
{
  "policy_version": "2026-04-01",
  "calibration_mode": "static_logged",
  "axes": [
    "scope",
    "abstraction",
    "context_span",
    "execution_proximity",
    "blast_radius",
    "coordination_load",
    "latency_pressure"
  ],
  "weights": {
    "macro": [1.2, 1.1, 0.9, -0.9, 0.8, 0.7, -0.3],
    "meso": [0.6, 0.5, 0.7, 0.1, 0.6, 1.0, 0.2],
    "micro": [-0.4, -0.6, -0.3, 1.3, 0.4, -0.2, 1.1]
  },
  "bias": {
    "macro": 0.0,
    "meso": 0.05,
    "micro": 0.0
  },
  "modifiers": {
    "observability_micro_multiplier": 0.25,
    "runtime_fit_multiplier": 0.20,
    "history_confidence_multiplier": 0.15
  },
  "thresholds": {
    "tie_margin": 0.08,
    "low_confidence": 0.12
  }
}
```

- [ ] **Step 4: Implement the scorer**

`packages/heiwa_cognition/heiwa_cognition/drex.py`:

```python
from __future__ import annotations

import json
import os
from pathlib import Path

from heiwa_protocol.drex import (
    DrexAuthorityGate,
    DrexDecision,
    DrexFailureMode,
    DrexModifiers,
    DrexScoreCard,
    DrexVector,
)


def default_policy_path() -> Path:
    env_override = os.environ.get("HEIWA_DREX_POLICY_PATH")
    if env_override:
        return Path(env_override)
    repo_candidate = Path.cwd() / "config" / "swarm" / "drex_policy.json"
    if repo_candidate.exists():
        return repo_candidate
    return Path(__file__).resolve().parents[3] / "config" / "swarm" / "drex_policy.json"


def build_drex_vector(intent: str, risk: str, raw_text: str, privacy: str, runtime: str) -> DrexVector:
    # Heuristic first slice. Authority is NOT part of the vector.
    ...


def evaluate_drex(
    intent: str,
    risk: str,
    raw_text: str,
    privacy: str,
    runtime: str,
    *,
    observability: float = 1.0,
    runtime_fit: float = 1.0,
    history_confidence: float = 0.5,
    policy_path: Path | None = None,
) -> DrexDecision:
    ...
```

Implementation rules:

- DREX stays at seven axes.
- `authority` is derived separately into `DrexAuthorityGate`.
- `observability` changes score confidence and penalizes micro when visibility is low.
- `calibration_mode` is stored in the loaded policy but not yet learned online.
- If the top two scores are within `tie_margin`, emit `DrexFailureMode.LOW_CONFIDENCE_TIE` and choose `meso` as the safe default unless the authority gate forces `macro`.
- Use `default_policy_path()` instead of a hard-coded repo-relative constant so Railway and site-packages installs can override with `HEIWA_DREX_POLICY_PATH`.

Heuristic guidance for `build_drex_vector()`:

- Start from a deterministic preset table keyed by intent:

| Intent family | scope | abstraction | context_span | execution_proximity | blast_radius | coordination_load | latency_pressure |
|--------------|------:|------------:|-------------:|--------------------:|-------------:|------------------:|-----------------:|
| `build`, `files` | 0.35 | 0.35 | 0.55 | 0.90 | 0.55 | 0.30 | 0.75 |
| `audit`, `status_check` | 0.45 | 0.40 | 0.60 | 0.30 | 0.45 | 0.25 | 0.60 |
| `research` | 0.70 | 0.80 | 0.85 | 0.20 | 0.40 | 0.65 | 0.30 |
| `strategy` | 0.90 | 0.95 | 0.80 | 0.10 | 0.75 | 0.85 | 0.20 |
| `deploy`, `operate`, `automate` | 0.75 | 0.60 | 0.65 | 0.65 | 0.90 | 0.70 | 0.70 |
| default | 0.50 | 0.50 | 0.50 | 0.50 | 0.50 | 0.50 | 0.50 |

- Then apply bounded adjustments:
  - `risk=high|critical` increases `blast_radius` and `scope` by `+0.15`, capped at `1.0`
  - `privacy=sovereign` increases `execution_proximity` by `+0.10` and decreases `scope` by `-0.05`
  - runtime `boost|macbook` increases `execution_proximity` by `+0.10`
  - long prompts (`len(raw_text) > 500`) increase `context_span` by `+0.10`
  - text containing `patch`, `edit`, `write`, `run`, `pytest`, `bash`, `shell` increases `execution_proximity` by `+0.10`
  - text containing `portfolio`, `enterprise`, `roadmap`, `priority`, `governance` increases `scope` and `abstraction` by `+0.10`

- Score calculation should be explicit:
  - `base_score = dot(weights[tier], vector) + bias[tier]`
  - micro score gets `- ((1 - observability) * observability_micro_multiplier)`
  - all scores may receive small additive terms from `runtime_fit` and `history_confidence`
  - confidence is the gap between the top two scores after modifiers

- Gate calculation should be explicit:
  - `approved_write` if `blast_radius >= 0.75`
  - `operator_review` if `scope >= 0.80` and `coordination_load >= 0.70`
  - otherwise `none`

- [ ] **Step 5: Export the scorer**

Modify `packages/heiwa_cognition/heiwa_cognition/__init__.py`:

```python
from heiwa_cognition.drex import evaluate_drex, build_drex_vector
```

- [ ] **Step 6: Run the scorer tests**

Run:

```bash
cd /Users/dmcgregsauce/heiwa
pytest apps/heiwa_hub/tests/test_drex_scoring.py -q
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add packages/heiwa_cognition/heiwa_cognition/drex.py \
  packages/heiwa_cognition/heiwa_cognition/__init__.py \
  config/swarm/drex_policy.json \
  apps/heiwa_hub/tests/test_drex_scoring.py
git commit -m "feat: add drex scoring policy"
```

---

### Task 3: Integrate DREX into the active router and route envelope

**Files:**
- Modify: `packages/heiwa_cognition/heiwa_cognition/router.py`
- Modify: `packages/heiwa_protocol/heiwa_protocol/routing.py`
- Modify: `apps/heiwa_hub/cognition/__init__.py`
- Test: `apps/heiwa_hub/tests/test_compute_router.py`
- Test: `apps/heiwa_hub/tests/test_compute_router_stdb.py`

- [ ] **Step 1: Write the failing router assertions**

Add to `apps/heiwa_hub/tests/test_compute_router_stdb.py`:

```python
class TestComputeRouterSTDB:
    ...

    def test_route_attaches_drex_decision(self):
        mock_stdb = MagicMock()
        mock_stdb.get_model_tiers.return_value = self._mock_tiers()
        router = ComputeRouter(stdb=mock_stdb)
        route = router.route("build", "medium")
        assert route.drex_decision is not None
        assert route.resolution_tier in {"macro", "meso", "micro"}
```

Add to `apps/heiwa_hub/tests/test_compute_router.py`:

```python
assert hasattr(route, "resolution_tier")
assert route.resolution_tier in {"macro", "meso", "micro"}
```

- [ ] **Step 2: Run the router tests and confirm they fail**

Run:

```bash
cd /Users/dmcgregsauce/heiwa
pytest apps/heiwa_hub/tests/test_compute_router_stdb.py -q
python apps/heiwa_hub/tests/test_compute_router.py
```

Expected: FAIL because `ComputeRoute` does not carry DREX fields yet.

- [ ] **Step 3: Extend the route dataclasses**

Modify `packages/heiwa_cognition/heiwa_cognition/router.py`:

```python
from heiwa_protocol.drex import DrexDecision, DrexFailureMode


@dataclass(slots=True)
class ComputeRoute:
    ...
    resolution_tier: str = "meso"
    drex_decision: DrexDecision | None = None
    drex_failure_modes: list[str] = field(default_factory=list)


@dataclass(slots=True)
class RoutedPlan:
    ...
    resolution_tier: str = "meso"
    drex_decision: DrexDecision | None = None
```

- [ ] **Step 4: Apply DREX before finalizing the route**

Implementation shape in `ComputeRouter.route()`:

```python
drex = evaluate_drex(
    intent=intent_class,
    risk=risk_level,
    raw_text=raw_text,
    privacy=result.privacy_level,
    runtime=result.target_runtime,
)
result.drex_decision = drex
result.resolution_tier = drex.scorecard.chosen_tier

if DrexFailureMode.LOW_CONFIDENCE_TIE in drex.failure_modes:
    result.rationale += " DREX tie resolved to safe meso default."
```

Guardrails:

- DREX must not break sovereign clamp behavior.
- DREX may escalate a lane upward, but it must not downgrade a required `heiwa_ops` path.
- If DREX chooses `macro`, preserve or raise `compute_class`; never silently lower it.
- If DREX chooses `micro` on Railway with a local-only model, keep the existing Railway guard and record a DREX execution mismatch failure.

- [ ] **Step 5: Add DREX fields to the broker envelope**

Modify `packages/heiwa_protocol/heiwa_protocol/routing.py`:

```python
@dataclass(slots=True)
class BrokerRouteResult:
    ...
    resolution_tier: str = "meso"
    drex_decision_json: str = "{}"
    drex_failure_modes: list[str] = field(default_factory=list)
```

Use `DrexDecision.to_dict()` at the boundary instead of leaking dataclass instances directly into payloads.

- [ ] **Step 6: Re-export through the hub compatibility layer**

Modify `apps/heiwa_hub/cognition/__init__.py` so imports such as `from heiwa_hub.cognition import DrexDecision` remain stable for app code.

- [ ] **Step 7: Run the router tests and existing regression checks**

Run:

```bash
cd /Users/dmcgregsauce/heiwa
pytest apps/heiwa_hub/tests/test_compute_router_stdb.py -q
python apps/heiwa_hub/tests/test_compute_router.py
pytest apps/heiwa_hub/tests/test_rate_group_routing.py -q
```

Expected: PASS, with existing route expectations still intact.

- [ ] **Step 8: Commit**

```bash
git add packages/heiwa_cognition/heiwa_cognition/router.py \
  packages/heiwa_protocol/heiwa_protocol/routing.py \
  apps/heiwa_hub/cognition/__init__.py \
  apps/heiwa_hub/tests/test_compute_router.py \
  apps/heiwa_hub/tests/test_compute_router_stdb.py
git commit -m "feat: attach drex decisions to routing"
```

---

### Task 4: Persist DREX decisions and failures to STDB

**Files:**
- Modify: `apps/heiwa_hub/spacetimedb/src/lib.rs`
- Modify: `packages/heiwa_sdk/heiwa_sdk/spacetimedb.py`
- Modify: `packages/heiwa_sdk/heiwa_sdk/spacetimedb.py:record_route_decision`
- Test: `apps/heiwa_hub/tests/test_drex_failure_taxonomy.py`

- [ ] **Step 1: Write the failing persistence test**

`apps/heiwa_hub/tests/test_drex_failure_taxonomy.py`:

```python
from unittest.mock import MagicMock

from heiwa_cognition.router import ComputeRouter


def test_router_records_drex_decision_and_failure():
    mock_stdb = MagicMock()
    mock_stdb.get_model_tiers.return_value = [
        {
            "model_id": "model-a",
            "capability_class": 2,
            "cost_per_turn": 0.0,
            "last_success_rate": 0.8,
            "effort_level": 1,
            "strengths_json": '["general"]',
            "enabled": True,
            "effort_knob": "none"
        }
    ]
    router = ComputeRouter(stdb=mock_stdb)
    route = router.route("build", "high", raw_text="high-risk patch with unclear observability")
    assert route.drex_decision is not None
    mock_stdb.insert_drex_decision.assert_called_once()
```

- [ ] **Step 2: Run the test and confirm it fails**

Run:

```bash
cd /Users/dmcgregsauce/heiwa
pytest apps/heiwa_hub/tests/test_drex_failure_taxonomy.py -q
```

Expected: FAIL because the STDB reducers and bridge methods do not exist.

- [ ] **Step 3: Add STDB tables and reducers**

Modify `apps/heiwa_hub/spacetimedb/src/lib.rs`:

```rust
#[table(accessor = drex_decisions, public)]
pub struct DrexDecisionRow {
    #[primary_key]
    pub decision_id: String,
    #[index(btree)]
    pub request_id: String,
    #[index(btree)]
    pub task_id: String,
    pub intent_class: String,
    pub risk_level: String,
    pub resolution_tier: String,
    pub scope: f64,
    pub abstraction: f64,
    pub context_span: f64,
    pub execution_proximity: f64,
    pub blast_radius: f64,
    pub coordination_load: f64,
    pub latency_pressure: f64,
    pub macro_score: f64,
    pub meso_score: f64,
    pub micro_score: f64,
    pub score_confidence: f64,
    pub requires_approval: bool,
    pub vector_json: String,      // keep full snapshots for replay/debug
    pub modifiers_json: String,   // keep full snapshots for replay/debug
    pub gate_json: String,        // keep full snapshots for replay/debug
    pub scorecard_json: String,   // keep full snapshots for replay/debug
    pub route_model: String,
    pub route_runtime: String,
    pub policy_version: String,
    #[index(btree)]
    pub created_at_ms: u64,
}

#[table(accessor = drex_failures, public)]
pub struct DrexFailureRow {
    #[auto_inc]
    #[primary_key]
    pub id: u64,
    #[index(btree)]
    pub decision_id: String,
    pub failure_mode: String,
    pub stage: String,
    pub details_json: String,
    pub recovered: bool,
    #[index(btree)]
    pub created_at_ms: u64,
}
```

Reducers:

- `insert_drex_decision(...)`
- `insert_drex_failure(...)`

Linkage rule:

- Add `#[default(None::<String>)] pub drex_decision_id: Option<String>` to the existing `RouteDecision` table.
- Extend `record_route_decision(...)` to accept an optional `drex_decision_id`.
- `drex_decisions.request_id` is the child link for replay/join safety.
- `route_decisions.drex_decision_id` is the direct join pointer for inspection and dashboards.

Schema note:

- Keep typed scalar columns for the seven axes and three tier scores so STDB queries such as "show all high blast-radius macro routes" remain possible without client-side JSON parsing.
- Keep the JSON snapshots as replay/debug payloads, not as the only stored representation.
- Use `created_at_ms: u64` for new DREX rows to keep ordering/range queries cheap without turning this plan into a repo-wide timestamp migration.

- [ ] **Step 4: Add bridge helpers**

Modify `packages/heiwa_sdk/heiwa_sdk/spacetimedb.py`:

```python
def insert_drex_decision(self, *, decision_id: str, request_id: str, task_id: str, intent_class: str, risk_level: str, resolution_tier: str, vector_json: str, modifiers_json: str, gate_json: str, scorecard_json: str, route_model: str, route_runtime: str, policy_version: str) -> bool:
    return self.call("insert_drex_decision", ...)


def insert_drex_failure(self, *, decision_id: str, failure_mode: str, stage: str, details_json: str, recovered: bool) -> bool:
    return self.call("insert_drex_failure", ...)
```

Also extend `record_route_decision()` to pass `drex_decision_id` when present so the existing control-plane log and the new DREX child row stay connected.

- [ ] **Step 5: Record decisions and failures from the router**

Modify `packages/heiwa_cognition/heiwa_cognition/router.py`:

```python
if self._stdb and hasattr(self._stdb, "insert_drex_decision"):
    self._stdb.insert_drex_decision(...)

for failure_mode in drex.failure_modes:
    if failure_mode != DrexFailureMode.NONE and hasattr(self._stdb, "insert_drex_failure"):
        self._stdb.insert_drex_failure(...)
```

Failure modes to emit in this first slice:

- `LOW_CONFIDENCE_TIE`
- `POLICY_GATE_OVERRIDE`
- `EXECUTION_MISMATCH`
- `POLICY_MISSING`
- `VECTOR_EXTRACTION_ERROR`
- `FALLBACK_ESCALATION`

- [ ] **Step 6: Publish the updated STDB module in local dev**

Run:

```bash
cd /Users/dmcgregsauce/heiwa/apps/heiwa_hub
STDB_SERVER="${STDB_SERVER:-local}" STDB_IDENTITY="${STDB_IDENTITY:-heiwaproductiondb}" \
  spacetime publish --server "$STDB_SERVER" "$STDB_IDENTITY"
```

Expected: local STDB module accepts the schema changes. If `STDB_SERVER` is not `local`, document that remote publish is handled by CI/manual deploy and skip this step.

- [ ] **Step 7: Run the failure taxonomy tests**

Run:

```bash
cd /Users/dmcgregsauce/heiwa
pytest apps/heiwa_hub/tests/test_drex_failure_taxonomy.py -q
```

Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add apps/heiwa_hub/spacetimedb/src/lib.rs \
  packages/heiwa_sdk/heiwa_sdk/spacetimedb.py \
  packages/heiwa_cognition/heiwa_cognition/router.py \
  apps/heiwa_hub/tests/test_drex_failure_taxonomy.py
git commit -m "feat: persist drex routing telemetry"
```

---

### Task 5: Verify DREX stays bounded and does not regress the current router

**Files:**
- Modify: `apps/heiwa_hub/tests/test_phase3_integration.py`
- Test: `apps/heiwa_hub/tests/test_phase3_integration.py`
- Test: `apps/heiwa_hub/tests/test_compute_router_stdb.py`
- Test: `apps/heiwa_hub/tests/test_rate_group_routing.py`

- [ ] **Step 1: Add an integration assertion for persisted DREX decisions**

Add to `apps/heiwa_hub/tests/test_phase3_integration.py`:

```python
def test_router_logs_drex_decision_on_route():
    mock_stdb = MagicMock()
    mock_stdb.get_model_tiers.return_value = [
        {
            "model_id": "gemini-cli/gemini-3-flash",
            "provider": "google-gemini-cli",
            "rate_group": "google_gemini_cli",
            "capability_class": 2,
            "effort_knob": "thinking:on",
            "effort_level": 4,
            "strengths_json": '["research","audit","status_check"]',
            "enabled": True,
            "cost_per_turn": 0.0,
            "last_success_rate": 0.95,
        }
    ]
    router = ComputeRouter(stdb=mock_stdb)
    route = router.route("audit", "low")
    assert route.drex_decision is not None
    mock_stdb.insert_drex_decision.assert_called_once()
```

- [ ] **Step 2: Verify full routing regression coverage**

Run:

```bash
cd /Users/dmcgregsauce/heiwa
pytest apps/heiwa_hub/tests/test_phase3_integration.py -q
pytest apps/heiwa_hub/tests/test_compute_router_stdb.py -q
pytest apps/heiwa_hub/tests/test_rate_group_routing.py -q
python apps/heiwa_hub/tests/test_compute_router.py
```

Expected: PASS.

- [ ] **Step 3: Smoke-check import stability**

Run:

```bash
cd /Users/dmcgregsauce/heiwa
python - <<'PY'
from heiwa_protocol import DrexDecision
from heiwa_cognition import evaluate_drex
from heiwa_hub.cognition import ComputeRouter
print("ok")
PY
```

Expected: prints `ok`.

- [ ] **Step 4: Record the follow-on work explicitly**

Add a short note to the task handoff or commit message:

- DREX weights are `static_logged`, not learned
- authority is intentionally outside the scored vector
- memory policy engine remains a separate spec
- Rust-native scorer remains deferred
- web/dashboard and Cloudflare projection work remain deferred

- [ ] **Step 5: Note deployment/runtime caveats**

Document in the handoff:

- Railway deploys need `HEIWA_DREX_POLICY_PATH` set if the policy file is not mounted at the repo-relative path
- remote STDB schema publish is not automatic from this plan
- DREX rows intentionally add typed scalar columns plus JSON snapshots

- [ ] **Step 6: Commit**

```bash
git add apps/heiwa_hub/tests/test_phase3_integration.py
git commit -m "test: verify drex routing integration"
```

---

## Execution Notes

- The first DREX rollout is successful when:
  - every route carries a `resolution_tier`
  - DREX decisions can be serialized and persisted
  - low-confidence or mismatched routes emit explicit failure records
  - existing router behavior still passes its current regression suite
- Do not implement learned weight adjustment in this plan. Logging and policy versioning are enough for the first slice.
- The next spec after this plan should be the deferred memory-policy engine, not another routing rewrite.
