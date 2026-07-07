# Heiwa Todo Routing Matrix

**Date:** 2026-06-06\
**Status:** Operator-private execution map\
**Source:** Review of the 18-item Odysseus/Hermes competitiveness backlog plus
Claude's E1-E10 inserts and routed subtask matrix.

## Verdict

The matrix is directionally right: the highest-value work is not more peer
comparison. It is proof that Heiwa's runtime moat works under pressure.

Keep these three as first-class proof gates:

1. **E3 DREX eval harness** — proves classification and route choice.
2. **E9 Receipt schema and read-side ergonomics** — makes receipts inspectable
   product evidence, not internal logs.
3. **E10 Provider-peer routing test harness** — proves routing survives provider
   unavailability, quota pressure, and local-first constraints.

Without those, Heiwa's moat is architecture. With those, it becomes a demo and
a regression suite.

## Review Findings

The routed build order is useful and mostly aligned with Heiwa's current
strategy. It correctly pulls forward public hygiene, observability, evals,
schema/versioning, backup/restore, update channels, privacy/consent, receipt
ergonomics, and provider-failure routing tests.

Important corrections:

1. **"Claude designs and decides" is too strong.** Claude can draft decisions,
   but repo/runtime truth and local verification decide.
2. **Local/Ollama should be explicit.** Cheap classification, tagging,
   summarization, and offline iteration should hit local models before remote
   provider lanes.
3. **Antigravity is availability-gated.** It is a target for background jobs,
   not a blocker lane until `heiwa providers` and execution proof show it is
   ready.
4. **Privacy/DPA work is a scaffold, not legal final.** It can define data
   inventory, subprocessors, deletion paths, and provider boundaries, but legal
   review remains separate before external users.
5. **STDB stays company backend.** SQLite remains local/internal app ops where
   useful; it does not replace Maincloud evidence/adjudication.

## Corrections To The Proposed Pattern

Claude should not be treated as the general decider. Repo truth decides.
Claude is strongest for product architecture, adversarial review, policy,
schema semantics, and deep design. Codex is strongest for repo edits, tests,
CLI/runtime implementation, and exact verification. Gemini is strongest for
large-context sweeps, source refresh, and visual/UI review. Antigravity is a
background lane for long jobs only after availability is proven.

Every routed item must return:

- exact files changed or read
- command receipts
- runtime/public truth if the item touches install, app, provider, STDB, or edge
- acceptance evidence, not only narrative

## Agent Fit Rules

| Agent        | Primary fit                                                                    | Avoid as primary when                                                          |
| ------------ | ------------------------------------------------------------------------------ | ------------------------------------------------------------------------------ |
| Claude       | architecture, policy, schema, product tradeoffs, adversarial review            | code requires many precise repo edits or local build iteration                 |
| Codex        | implementation, tests, CLI/runtime work, JSON/schema/doc patches, verification | task mainly needs long synthesis or visual inspection                          |
| Gemini       | long-context sweeps, broad source review, UI/visual checks, multimodal review  | task needs narrow code edits under local repo conventions                      |
| Antigravity  | long background runs, soak tests, queueable async checks                       | provider availability is unverified or result needs tight interactive judgment |
| Local/Ollama | naming, tagging, classification, summarization, cheap offline draft iteration  | task needs external current facts, deep code edits, or high-stakes judgment    |

## Priority Build Order

1. E1 pre-public-push hygiene gate
2. H1 release/install truth
3. H2 app/runtime API truth
4. E2 runtime observability
5. E3 DREX eval harness
6. E10 provider-peer routing test harness
7. H4 one full loop with approvals and receipt
8. H5 approval surfaces
9. E9 receipt schema and read-side ergonomics
10. H6 native app wrapper
11. H7 desktop basics
12. H8 durable memory/read model
13. E4 STDB schema versioning
14. E5 backup/restore and Maincloud sync
15. H11 first product-grade connector
16. H12 first gateway intake
17. H13 model cookbook-lite
18. H14 compare/research minimum
19. H9 skill/procedure loop
20. H10 scheduler
21. H15 MCP/tool catalog
22. H16 sandboxed computer use
23. H17 billing/entitlement
24. E6 update channels
25. E7 privacy/DPA scaffold
26. E8 telemetry consent UX
27. H18 multi-device/team
28. Final peer-parity refresh against Odysseus/Hermes

## Routing Matrix

Machine-readable source of truth: [`config/swarm/heiwa_todo_routing_matrix_v1.json`](../../config/swarm/heiwa_todo_routing_matrix_v1.json).

| ID  | Plane     | Item                                         | Primary | Secondary | Acceptance Proof                                                                                |
| --- | --------- | -------------------------------------------- | ------- | --------- | ----------------------------------------------------------------------------------------------- |
| H1  | Evidence  | Release/install truth                        | Codex   | Claude    | public/private-alpha install state is exact; release/checksum path proven or explicitly blocked |
| H2  | Intake    | App/runtime API truth                        | Codex   | Gemini    | `/api/v1/*` returns typed JSON or explicit 404; no SPA fallback on API miss                     |
| H3  | Evidence  | Remove Devon-only leakage from product views | Codex   | Gemini    | inbox/history expose portable `SourceRef`/`ReceiptRef`, not old local archive paths             |
| H4  | Execution | One full loop with approvals and receipt     | Codex   | Claude    | ask -> classify -> route -> execute/stage -> receipt -> app display works end to end            |
| H5  | Execution | Approval surfaces                            | Codex   | Claude    | risky action packet can be approved/denied from CLI and app with receipt                        |
| H6  | Intake    | Native-feeling Heiwa.app wrapper             | Codex   | Gemini    | macOS local app launches runtime and cockpit over same state                                    |
| H7  | Intake    | Desktop basics                               | Codex   | Gemini    | chat, provider setup, live activity, artifact preview, settings, diagnostics                    |
| H8  | Evidence  | Durable memory/read model                    | Codex   | Claude    | readable local records with source spans, freshness, and safe export                            |
| H9  | Evidence  | Skill/procedure loop                         | Claude  | Codex     | completed work can propose a reviewed, evidence-backed procedure                                |
| H10 | Execution | Scheduler                                    | Codex   | Claude    | scheduled job runs under policy and emits receipt                                               |
| H11 | Intake    | First product-grade connector                | Codex   | Claude    | auth, list, bounded action, revoke, tests, receipts                                             |
| H12 | Intake    | First gateway intake                         | Codex   | Gemini    | one external channel normalizes into `InboxItem`; outbound is staged draft                      |
| H13 | Execution | Model cookbook-lite                          | Codex   | Gemini    | hardware/model probe recommends and verifies local model routes                                 |
| H14 | Evidence  | Compare/research minimum                     | Codex   | Gemini    | model compare or research flow produces source-linked report                                    |
| H15 | Execution | MCP/tool catalog                             | Codex   | Claude    | manifests define scopes, trust class, leases, tests                                             |
| H16 | Execution | Sandboxed computer use                       | Codex   | Gemini    | browser/file/computer actions carry screenshot/action trace and approval gates                  |
| H17 | Evidence  | Billing/entitlement                          | Claude  | Codex     | local entitlement readout and STDB mirror path exist without browser-secret leakage             |
| H18 | Evidence  | Multi-device/team groundwork                 | Claude  | Codex     | shared evidence/approval schema has local-first boundary and migration plan                     |
| E1  | Evidence  | Pre-public-push hygiene gate                 | Codex   | Gemini    | tracked-tree secret scan, public-surface audit, release sandbox proof                           |
| E2  | Evidence  | Runtime observability                        | Codex   | Claude    | local runtime status includes workers, providers, app, hooks, receipts, STDB, update channel    |
| E3  | Execution | DREX eval harness                            | Codex   | Claude    | golden evals prove intent/risk/route decisions and fail on regressions                          |
| E4  | Evidence  | STDB schema versioning                       | Claude  | Codex     | reducer/table versions, migration notes, generated binding compatibility gate                   |
| E5  | Evidence  | Backup/restore plus Maincloud sync           | Codex   | Claude    | local restore drill and STDB narrow-sync receipt proof                                          |
| E6  | Evidence  | Update channels                              | Codex   | Claude    | stable/beta/dev channel semantics and rollback proof                                            |
| E7  | Evidence  | Privacy/DPA scaffold                         | Claude  | Gemini    | public-safe privacy boundary, DPA draft, data inventory, subprocess/provider ownership map      |
| E8  | Intake    | Telemetry consent UX                         | Gemini  | Codex     | opt-in/off UI plus local-only default and evidence of no hidden home-call                       |
| E9  | Evidence  | Receipt schema/read ergonomics               | Codex   | Claude    | receipts are queryable by run, source, provider, approval, artifact, and chain status           |
| E10 | Execution | Provider-peer routing harness                | Codex   | Gemini    | tests cover provider down, quota low, local-only, cost-first, and model unavailable cases       |

## Immediate Implementation Slice

Implement E3, E9, and E10 before broad connector or desktop expansion. They make
the moat measurable, and they reduce risk for every later feature.

First slice:

1. Add DREX route eval fixtures for current `heiwa route preview` behavior.
2. Add provider failure/quota fixtures that assert local-first fallback.
3. Add receipt read ergonomics for run/provider/approval/source/chain status.

Do not start a generic service offer, connector sprawl, or hosted control plane
until these proof gates are green.

## Subtask Routing Notes

Use these as the first split when a macro item starts:

- **E1**: Claude drafts security/public-readiness gate; Codex writes workflow
  YAML/scripts; Gemini sweeps public text and repo path leaks.
- **H1**: Claude owns public/private-alpha announcement and signing custody;
  Codex owns release pipeline, installer, `doctor`, and checksum proof.
- **H2**: Gemini audits `/api/v1/*`; Claude designs typed contract; Codex
  removes SPA fallback and implements provider-state reconciliation.
- **H3**: Gemini finds hardcoded/personal leaks; Claude defines portable
  `SourceRef`, `ReceiptRef`, `InboxItem`; Codex refactors read paths.
- **H4/E3**: Claude designs demo/eval cases; Codex implements classifier
  fixtures and UI/run/receipt path; Antigravity may run batch evals after
  provider availability is proven.
- **H5/E9**: Claude defines approval and receipt contracts; Codex builds CLI,
  app, persistence, read helpers, export, and redaction tests.
- **H6/E6**: Claude decides update safety/channel semantics; Codex builds app
  scaffold, manifests, rollback, and package proof; Antigravity may run
  signing/notarization CI when credentials are present.
- **H7/E2/E8**: Gemini supplies UI/reference review; Claude defines redaction
  and consent boundaries; Codex builds streaming, settings, diagnostics,
  observability, and consent UX.
- **H8/E5**: Claude owns memory/sync semantics; Codex builds FTS/vector read
  model, freshness, backup/restore drills, and Maincloud narrow-sync proof.
- **H9/H10**: Claude designs skill and scheduler policy; Codex builds review
  gate, activation, cron/app surfaces, and execution receipts.
- **H11/E7**: Claude picks first connector and drafts privacy/DPA scaffold;
  Codex builds OAuth/list/read/action/revoke/receipts; Gemini generates broad
  fixtures and boilerplate for review.
- **H12**: Claude defines channel choice and no-raw-command safety; Codex
  normalizes inbound to `InboxItem` and stages outbound drafts.
- **H13/H14/E10**: Gemini sweeps model/benchmark data; Claude designs scoring
  and failure scenarios; Codex builds hardware/model probes, compare/research,
  and provider-peer fault-injection harness.
- **H15-H18/E4**: Claude defines manifests, leases, sandbox policy, entitlement,
  multi-device/team schema, and STDB migration policy; Codex implements gates,
  generated bindings, local readouts, and tests.
