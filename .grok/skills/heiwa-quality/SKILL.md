---
name: heiwa-quality
description: Raise Heiwa quality without stalling progress—policy, receipts, tests, privacy, and agent-facing docs. Use when reviewing, hardening, or the user asks for quality bar / refine quality.
argument-hint: "[surface or PR focus]"
user-invocable: true
disable-model-invocation: false
---

# /heiwa-quality — refine quality while still shipping

## Quality bar (must not regress)

- [ ] Plane classification clear
- [ ] Fail-closed policy (no silent external writes)
- [ ] Receipts / source spans where side effects exist
- [ ] No AGPL/GPL code into Apache core without explicit decision
- [ ] Secrets not logged or committed
- [ ] Tests for new pure policy / schema code
- [ ] Docs updated for contracts (`docs/`)

## Method

1. Inventory risks in the touched surface (privacy, data loss, policy bypass).
2. Add/adjust tests first when fixing policy bugs.
3. Prefer small hardening PRs over rewrites.
4. If something is demo-ware, either delete, mark target, or complete the contract.

## Inference

Use local/free models for checklist review; escalate to Claude/Grok only for subtle security design.

$ARGUMENTS
