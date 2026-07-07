# Heiwa OSS Lift Matrix — Product vs Company/Local DevOps

Date: 2026-06-14 America/Vancouver
Operator: Devon McGregor

## Scope

Classify OSS sources into one of two destinations:

1. **Heiwa shipped product** — code or algorithms that should become part of the local-first Heiwa runtime/app/CLI, with receipts, scopes, approvals, and no user-facing model picker.
2. **Heiwa Limited / Devon local devops** — OSS used to run, verify, publish, or monitor GitHub, Cloudflare, and SpacetimeDB infrastructure. These should stay operator tooling unless a product use case appears.

## Evidence gathered

- `~/oss-repos` is the raw upstream git-clone area.
- `~/heiwa-universe/vendor/oss-*` is the assigned translated/lifted-code staging area before intentional promotion into the product spine.
- This corrected copy lives under `~/heiwa-universe/vendor/oss-lifts/reports/`; the older ad-hoc copy under `~/heiwa-universe/oss-lifts` is report output only, not a source/lift location.
- Existing source inventory is under `~/oss-repos`:
  - `activepieces`, `agent-skills`, `browser-use`, `cal.diy`, `caldav`, `dateparser`, `headroom`, `hermes-agent`, `huginn`, `impeccable`, `inbox-zero`, `khoj`, `letta`, `litellm`, `LLMLingua`, `mem0`, `mlx-lm`, `pipali`, `Rapid-MLX`, `recurrent`, `RouteLLM`, `ruflo`, `the-book-of-secret-knowledge`.
- `https://github.com/dmtrKovalenko/fff` was cloned to `/tmp/heiwa-oss-lift-fff` for inspection.
- FFF test probe: `~/.cargo/bin/cargo test -p fff-query-parser --locked` passed: **82 unit tests + 3 doctests**.
- Heiwa spine inspected:
  - `~/heiwa-universe/AGENTS.md`
  - `~/heiwa-universe/HEIWA.md`
  - `~/heiwa-universe/docs/oss-lift-integration-plan.md`
  - `~/heiwa-universe/docs/deployment.md`
  - `~/heiwa-universe/docs/current-capability.md`
  - `~/heiwa-universe/crates/heiwa_mcp/src/local_tools.rs`
  - `~/heiwa-universe/crates/heiwa_automations/src/*`
  - `~/heiwa-universe/crates/heiwa_receipts/src/lib.rs`
  - `~/heiwa-universe/.github/workflows/{deploy,pages,release}.yml`
- Heiwa checkout is dirty on `main`; I did **not** modify product repo files.

## License rule

- **MIT / BSD / Apache-2.0**: direct code lift is allowed with attribution/notice handling.
- **Dual GPL/Apache**: use the permissive side only when the project explicitly offers it, e.g. `caldav` is GPL-3.0 **or** Apache-2.0.
- **AGPL / commercial-use-restricted AGPL**: no code in Heiwa. Patterns only, clean-room reimplementation.
- **No license / NOASSERTION**: do not copy code; use only as installed tool or external service if its terms allow it.

## Product lifts: put inside Heiwa shipped runtime/app

| Priority | Source | License verdict | Product asset | Heiwa target | Action |
|---:|---|---|---|---|---|
| 1 | `dmtrKovalenko/fff` | MIT, direct lift allowed | Fast repo file/content search, frecency, query parser, git annotations, MCP tool shape | `crates/heiwa_mcp/src/local_tools.rs`, `apps/heiwa_shell/src/agentic.rs`, `/api/v1/capabilities` | Replace naive `repo.grep`; add `repo.find`; keep `ExecutionScope` and tool leases as authority. |
| 2 | `dateparser` + `recurrent` | BSD-3 + MIT | NL time / recurrence parsing | Existing `heiwa schedule` path | Already integrated per `docs/oss-lift-integration-plan.md`; keep as deterministic Intake parser. |
| 3 | `cal.diy` | MIT | slots/date ranges/availability + connector skeletons | Calendar read model / future `heiwa_calendar` crate | Port algorithms when Apple/Google free-busy read model lands. Do not lift UI. |
| 4 | `caldav` | dual GPL-3.0 or Apache-2.0 | CalDAV client library semantics | Calendar connector lane | Use Apache-2.0 side; preferably dependency/adapter, not source copy. |
| 5 | `pipali` | Apache-2.0 | local automation scheduler, MCP OAuth provider, file watcher ergonomics | `crates/heiwa_automations`, Heiwa.app Automations view | Lift patterns only where they fill existing crate gaps. Heiwa already has deterministic cron/file-watch primitives. |
| 6 | `headroom` | Apache-2.0 | compression policies, detectors, model limits, cache drift/volatile detection | `crates/heiwa_receipts`, `apps/heiwa_shell/src/cmd/compress.rs`, DREX context economy | Lift missing compression/detector pieces; do not duplicate the existing SQLite receipt chain. |
| 7 | `impeccable` | Apache-2.0 | deterministic frontend anti-pattern detector + contrast/color rules | Heiwa.app CI/design gate | Direct lift or reimplement detector catalog as a design-regression script. High value because it catches known Heiwa UI regressions. |
| 8 | `Rapid-MLX` / `mlx-lm` | Apache-2.0 / MIT | local Apple Silicon inference | local model provider lane | Integrate as sidecar/subprocess, not vendored product code. Heiwa wraps local runtimes; it does not become them. |
| 9 | `RouteLLM` / `litellm` | Apache-2.0 / MIT | router eval patterns, provider/pricing metadata | DREX `ModelTier`, provider registry | Reference and data-shape lift only. DREX remains the router. No user-facing model picker. |
| 10 | `browser-use` | MIT | browser control patterns and security tests | future Browser/computer-use lane | Reference first. Productize only behind leases/approvals/receipts. |
| 11 | `mem0` / `letta` | Apache-2.0 | memory abstractions, long-running agent state | after receipts/source-spans stabilize | Later. Receipts and source-spans must be the memory substrate first. |
| 12 | `inbox-zero`, `khoj` | AGPL / AGPL-like; no code | mail triage, automations, date filters, agent UX patterns | Mail/Automations clean-room implementations | Patterns only. Do not copy source into Heiwa. |

## Local devops lifts: use for Heiwa Limited / Devon ops, not shipped product

| Priority | Source | License verdict | Devops use | Target surface | Action |
|---:|---|---|---|---|---|
| 1 | `fff-mcp` | MIT | Faster local repo search for AI agents working in `~/heiwa-universe` | local agent/MCP config | Install/configure only with explicit operator approval because it mutates live tool config. Product code should still use scoped in-process tools. |
| 2 | `rhysd/actionlint` | MIT | Static check GitHub Actions YAML | `scripts/check_agent_baseline.sh` or new `scripts/check_ci_workflows.sh` | Add local-only preflight. No product dependency. |
| 3 | `woodruffw/zizmor` | MIT | GitHub Actions security lint | same as above | Add local/CI optional security audit for release workflows. |
| 4 | `cloudflare/workers-sdk` + `cloudflare/wrangler-action` | Apache-2.0 | Cloudflare Pages/Workers deploy tooling | `.github/workflows/deploy.yml`, local Cloudflare preflight | Existing workflow already uses `cloudflare/wrangler-action@v3`; keep official path. No custom Cloudflare deploy code in product. |
| 5 | `clockworklabs/SpacetimeDB` | GitHub API reports NOASSERTION; treat as external tool | Official `spacetime login/show/publish` shell path | `crates/heiwa_stdb`, release/publish runbooks | Do not lift code. Wrap CLI probes/publish preflight only. Preserve official Maincloud auth flow. |
| 6 | `axodotdev/cargo-dist` | Apache-2.0 | Release packaging patterns | `.github/workflows/release.yml`, `scripts/package_release_sandbox.sh` | Reference or adopt only if it simplifies current release assets without weakening local sandbox authority. |
| 7 | `goreleaser/goreleaser` | MIT | Cross-platform release packaging reference | release workflow | Reference only; current Rust custom workflow is already clear. |
| 8 | `release-plz` | Apache-2.0 | Rust crate release automation | not current product need | Defer. Heiwa app/runtime release > crate publishing. |
| 9 | `crate-ci/typos` | Apache-2.0 | Cheap docs/code typo gate | local baseline / docs CI | Safe optional devops gate. |
| 10 | `activepieces`, `huginn` | MIT-ish/local license says MIT for both inspected repos; activepieces GitHub API reports NOASSERTION | Workflow/event automation inspiration for ops monitors | local Heiwa Limited ops notebooks/scripts | Reference only; too large and hosted-workflow-shaped for product code. |

## FFF deep-dive verdict

**Verdict:** `fff` is the highest-value new candidate from this pass.

Facts:

- Repo: `dmtrKovalenko/fff`
- GitHub API at scan time: ~8,541 stars, MIT, Rust, updated 2026-06-15T06:46:25Z.
- Core inspected files:
  - `/tmp/heiwa-oss-lift-fff/crates/fff-core/src/lib.rs`
  - `/tmp/heiwa-oss-lift-fff/crates/fff-core/src/file_picker.rs`
  - `/tmp/heiwa-oss-lift-fff/crates/fff-core/src/grep.rs`
  - `/tmp/heiwa-oss-lift-fff/crates/fff-core/src/dbs/frecency.rs`
  - `/tmp/heiwa-oss-lift-fff/crates/fff-mcp/src/server.rs`
- MCP tools exposed upstream:
  - `find_files`
  - `grep`
  - `multi_grep`
- Product-relevant behavior:
  - background index + watcher
  - typo/fuzzy path search
  - content search with plain/regex/fuzzy modes
  - definition/import line classification
  - frecency decay tuned for AI sessions
  - git-aware result annotations
  - cursor pagination
  - LLM-friendly parameter normalization

### Best Heiwa product shape for FFF

Do **not** ship `fff-mcp` as a black-box product sidecar first. Heiwa already has its own scoped MCP/tool registry:

- `crates/heiwa_mcp/src/local_tools.rs` currently registers `fs.read`, `fs.list`, `repo.grep`.
- `repo.grep` is currently a simple literal recursive search inside `ExecutionScope`.
- `apps/heiwa_shell/src/agentic.rs` tool prompt only advertises `fs.list|fs.read|repo.grep`.
- `apps/heiwa_shell/src/cmd/app.rs` surfaces these tools through `/api/v1/capabilities`.

So the first product slice should be:

1. Add a `repo.find` read-only tool.
2. Replace the backend of `repo.grep` with FFF-style indexed search.
3. Preserve Heiwa authority:
   - all paths must remain inside `ExecutionScope`
   - tool lease required
   - no hidden network
   - no mutation
   - result receipts/tool transcript stays Heiwa-owned
4. Add attribution/notice for MIT source if code is copied.

### Why this beats using `ripgrep`/`fd` directly

`ripgrep` and `fd` are excellent dev tools, but `fff` is already shaped for agentic repeated search:

- long-running process index instead of cold-start one-shot CLI
- frecency and git-status signals
- LLM-specific tool schemas and pagination
- query parser with constraint syntax and fuzzy fallback
- direct Rust library surface, not only shell output

Use `ripgrep`/`fd` as terminal fallbacks; use FFF concepts/code for Heiwa's product file context layer.

## First executable product slice

**Name:** FFF-backed Heiwa repo context tools

**Plane:** Intake + Evidence. It captures repo/file context and makes every search scope/receipt inspectable.

**Files likely touched:**

- Modify: `~/heiwa-universe/crates/heiwa_mcp/Cargo.toml`
- Modify: `~/heiwa-universe/crates/heiwa_mcp/src/local_tools.rs`
- Modify: `~/heiwa-universe/apps/heiwa_shell/src/agentic.rs`
- Modify: `~/heiwa-universe/apps/heiwa_shell/src/main.rs`
- Modify: `~/heiwa-universe/apps/heiwa_shell/src/cmd/app.rs`
- Tests: `~/heiwa-universe/crates/heiwa_mcp/tests/*` or inline tests in `local_tools.rs`
- License notice: root `LICENSE`/third-party notices surface, exact file TBD after checking existing license-notice convention.

**Acceptance:**

- `repo.find {query,path,max_matches}` returns ranked files only inside scope.
- `repo.grep {pattern,path,max_matches}` supports current literal behavior plus FFF-like fuzzy/smart fallback without breaking existing callers.
- `/api/v1/capabilities` shows `repo.find` with `host_safe_readonly` and `lease_required: true`.
- Agentic prompt advertises `repo.find` and still keeps tools narrow.
- Tests prove outside-scope paths are denied.
- Tests prove binary/large files are capped/ignored the same way as current product policy.
- `cargo test -p heiwa-mcp` passes.
- Relevant `heiwa-shell` tests around capabilities/tool prompt pass.

**Out of scope for first slice:**

- No Cloudflare/GitHub/STDB mutation.
- No global MCP config mutation.
- No daemon install.
- No user-facing search UI redesign.
- No full-text persistent index for mail/calendar/docs.

## First executable local-devops slice

**Name:** protected backend preflight bundle

**Plane:** Heiwa Limited / Devon local devops, not product.

**Purpose:** make GitHub/Cloudflare/STDB operational safety cheap before publishing.

**Likely pieces:**

1. GitHub Actions static lint:
   - `actionlint`
   - `zizmor`
2. Cloudflare dry-run/preflight:
   - confirm `ENABLE_CLOUDFLARE_DEPLOY`
   - confirm `CLOUDFLARE_API_TOKEN`/account presence without printing secrets
   - verify static build output path before `wrangler pages deploy`
3. SpacetimeDB preflight:
   - `spacetime login show`
   - `spacetime publish --help`/module build check only
   - no production publish unless explicitly approved
4. Release provenance:
   - verify Git tag, release assets, checksums, docs build
   - keep GitHub Releases as binary/source authority

**Out of scope:** changing production Cloudflare/STDB/GitHub state without explicit assignment.

## Red-line decisions

- Do not vendor AGPL code from `khoj` or `inbox-zero`.
- Do not turn Cloudflare into binary authority.
- Do not create a hosted Heiwa runtime/control plane through these lifts.
- Do not expose a normal-user model picker while adopting `RouteLLM`, `litellm`, or Rapid-MLX data.
- Do not lift upstream UI layouts that violate Heiwa.app rules; lift algorithms and evidence shapes only.

## Recommended order

1. **Product:** FFF-backed `repo.find` + improved `repo.grep` behind Heiwa's existing tool leases.
2. **Devops:** add local GitHub Actions/static workflow audit (`actionlint` + `zizmor`) and a Cloudflare/STDB preflight script.
3. **Product:** `impeccable` design-regression gate for Heiwa.app.
4. **Product:** calendar connector/read-model work from `cal.diy` + `caldav` once the current schedule/approval loop needs real free-busy.
5. **Product:** headroom-style compression/detectors only where receipts/context budget hits a real bottleneck.
