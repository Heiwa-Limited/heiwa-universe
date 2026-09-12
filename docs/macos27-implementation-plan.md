# macOS 27 desktop implementation and local installation

2026-09-11. Scope: Intake, Execution, Evidence. Local development and installation
are authorized; publication is separate. User target is `~/Applications/Heiwa.app`.

## Product decisions

The installed TypeScript/Solid + Tauri application is the delivery surface. Rust
remains the authority for conversation history, projects, execution and evidence.
The permanent composer follows the selected session across Home, Calendar, Mail
and other views. The response panel expands into that same session. Projects are
optional collections of sessions and context, distinct from repository workspaces
and durable Work execution identities.

## Immediate acceptance boundary

1. Persist named projects and standalone/project sessions through the existing
   Rust operator journal/service. Preserve old sessions and their IDs. Support
   create, rename, move and archive without deleting history. Project events must
   never appear as fabricated chat sessions. Validate metadata before append.
2. Replace the icon-only shell with a navigable chat/work sidebar: Home, Calendar,
   Mail, a session list and project groups. Show loading, empty and failure states.
   Restore a valid last-selected session, retain separate drafts, and prevent late
   stream frames from appearing in another session. Cancelling observation must
   not cancel the underlying turn.
3. Build the native macOS 27 app with its bundled Rust runtime. Test changed
   behavior, inspect the bundle, verify on an isolated runtime/profile, preserve
   the previous installed app for rollback, update the actual shortcut target,
   and verify the launched executable. Inspect active work before runtime update.

## Shared transport contract

All routes retain the existing authenticated local operator API boundary.

- `GET /api/v1/operator/catalog` returns `{ok:true,data:{threads,projects}}`.
  Threads include the existing summary fields plus `title: string | null`,
  `project_id: string | null`, and `archived: boolean`. Projects are
  `{project_id:string,title:string,archived:boolean}`. Unknown metadata defaults
  to a standalone unarchived session. A bounded response reports truncation.
- `POST /api/v1/operator/projects` accepts `{project_id?:string,title:string}`
  and returns `{ok:true,data:{project}}`.
- `POST /api/v1/operator/projects/{id}/metadata` accepts optional `title` and
  `archived` and returns the updated project.
- Existing `POST /api/v1/operator/threads` accepts optional `thread_id`, `title`,
  and nullable `project_id`, preserving the existing response shape.
- `POST /api/v1/operator/threads/{id}/metadata` accepts optional `title`, nullable
  `project_id`, and `archived`, and returns `{ok:true,data:{thread}}`.
  Omitted fields remain unchanged; null project detaches the session. Invalid or
  unknown project IDs are rejected without partial metadata changes.
- The existing history, turn submission, Work and event cursor contracts remain
  authoritative. Subscribe teardown is explicit and window-scoped. UI preferences
  may remember a selected ID; they do not constitute canonical session storage.

## Execution routing for this implementation

The controller retains architecture, unresolved decisions, integration and final
review. Terra medium receives bounded backend and frontend ownership with this
contract and executable acceptance criteria. Tools handle deterministic searches,
builds and checks. Escalate if acceptance fails or ambiguity cannot be resolved;
do not repeatedly spend cheap calls on the same unresolved failure. Record actual
model/effort selections, results, retries and available usage without inventing
savings or transferring hidden reasoning.

## Subsequent product gates

The complete product still needs structured current-view/selection context at
the turn boundary, durable attachments and capability-gated multimodal transport,
Apple resource read/write workflows, and measured model/effort routing. Each must
connect UI -> Rust policy -> provider/connector -> persistent result and failure
recovery. Keep capability claims tied to those executable paths. Provider adapters
must retain channel/account/billing distinctions and apply the same acceptance
and total-cost policy, with native effort controls only where supported.

### Provider-independent engineering economics

`crates/heiwa_drex/src/lib.rs` currently favors the cheapest model clearing a
capability class. That is a starting heuristic, not measured cost per accepted
result. Extend the shared execution planner rather than embedding another router
in the desktop. The next routing implementation has five ordered gates:

1. Build a typed task assessment from user intent and admitted context: required
   modalities/tools, output acceptance checks, consequence, ambiguity, deadline,
   privacy and budget. Known deterministic transformations go directly to tools.
2. Filter exact account/channel/model candidates by current authentication,
   capability evidence, allowance, supported native effort and privacy constraints.
   Missing price, quota or effort evidence remains unknown, never silently zero.
3. Compare model and effort choices together while recording them separately.
   Include input/output tokens, tool fees, expected retries, review, handoff and
   latency. Initially use a declared conservative policy; only use estimated
   acceptance probability when backed by enough comparable completed tasks.
4. Execute a bounded assignment with exclusive write ownership, result schema,
   cancellation, retry ceiling and acceptance checks. Escalate after a meaningful
   failed check or unresolved ambiguity. Changing provider cannot repeat an
   external effect without its existing idempotency/evidence boundary.
5. Record requested and observed model/effort, channel, token-count provenance,
   elapsed time, retries and accepted/failed outcome with the existing route/usage
   receipts. Evaluate total cost and rework on equivalent tasks. Provider adapters
   translate native controls; they do not choose budgets or fabricate effort.

Desktop presentation should say what is working and why a route was chosen. A
detailed inspector can show the assignment, usage and acceptance receipts. The
user should not have to micromanage provider/model selectors for routine work.

Direct downloads do not require an App Store listing. Local development and
ad-hoc builds can proceed without paid Apple enrollment. Developer ID signing and
notarization for conventional public Gatekeeper trust are a later distribution
gate; no DocuSign step is involved.

## Visual revision and execution review

The first installed revision was rejected by the user. Follow
`docs/macos27-visual-direction.md` for the replacement: a dark native window,
readable sidebar and continuous app surfaces, meaningful session/project
navigation, and a persistent composer with an anchored response panel. The
Swift design study supplies proportions only, never fabricated runtime data.

Terra medium produced the initial visual rewrite but returned incomplete
lifecycle controls and skipped regression checks. Those returns failed
acceptance. The controller took over native chrome and visual integration;
Sol medium received the bounded lifecycle-control and regression work. This
is a recorded escalation after rework, not evidence of measured cost savings.
Model pricing and per-agent usage were unavailable for this assignment.

The desktop change can reuse the running, already verified Rust service when
its bundled runtime hash is unchanged. Visual acceptance requires inspection
of the actual installed macOS window, not just a successful Vite build.

## Local verification of the replacement

The replacement was built and installed from this uncommitted experimental
checkout on September 11, 2026. The installed shortcut resolves to the per-user
app bundle. Both native app and Rust CLI hashes were checked against build
outputs, and the idle LaunchAgent was restarted after the Mail runtime repair.
The app remains a local ad-hoc build, not a published or notarized release.

- Frontend: 114 Vitest tests and 9 packaging tests passed; TypeScript check passed.
- Native Mail bridge: 6 focused tests passed; desktop Clippy with warnings denied
  passed. No live Mail read or provider inference was performed.
- Mail snapshot: 20 unit tests and 3 triage integration tests passed. Regressions
  first reproduced stale unread state and corruption being accepted as success.
  Updates now preserve unqueried rows and use a locked, private atomic replacement.
- Installed UI inspection: dark Home at default and 1000 x 680 sizes; Calendar
  connection controls and accessible date table; anchored response panel; Mail
  action; resource-to-Mail navigation; session rename menu/dialog and Escape focus
  restoration; Command-L composer focus. Empty states reflect real local data.
- Experimental baseline passed with `--allow-dirty`; remote health was not
  checked. Local build/install evidence and hashes are in the ignored
  `target/macos27-verification/final-install.json` receipt. User acceptance of the
  revised visual direction is still separate from these checks.

The Mail permission/account flow still needs live macOS permission and source-app
verification. Other open product work remains the structured turn context,
attachments/media transport, deeper Apple actions, measured routing, and public
fresh-machine installation certification described above.
