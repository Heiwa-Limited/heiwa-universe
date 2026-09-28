# C1-a enforcement spike and remaining implementation

Date: 2026-09-27
Status: Offline canary spike verified locally; C1-a implementation and engine admission remain incomplete.
Planes: Execution, Evidence
Design: `docs/superpowers/specs/2026-09-27-heiwa-engines-design.md`

## Result

An outer macOS Seatbelt profile can deny synthetic authority-file reads,
approval-file writes, writes outside a workspace, symlink access, and loopback
TCP connections while allowing workspace reads and writes. A detached child
inherits the restrictions in these tests. An Apple Event policy query reports
denial; no Apple Event was sent and no TCC permission was requested.

This demonstrates kernel denial mechanics for controlled fixtures. It does not
demonstrate a usable Claude engine. The candidate permits broad host reads
outside the protected fixture and denies all networking. It must not become a
production sandbox profile or an engine admission certificate.

The initially tried deny-default profile aborted even a harmless process before
it could run the probe. That result was not containment evidence: its positive
control failed. A viable production allowlist still needs to admit provider
startup, authentication, inference, and scoped tool communication.

## Reproduce

```sh
python3 -m unittest discover -s scripts/tests -p test_engine_enforcement_probe.py -v
python3 scripts/probe_engine_enforcement.py
```

The runner uses Python's standard library and the host's existing
`/usr/bin/sandbox-exec`. It creates synthetic files and an ephemeral loopback
listener, runs controlled subprocesses with a cleared environment, and removes
the fixtures afterward. It never opens the installed Heiwa state, real
credentials, provider sessions, or productivity applications. It performs no
inference. Unsupported hosts report `unsupported` with a nonzero exit code.

A private receipt under `private/verification/enforcement-*/receipt.json`
records source revision and dirty state, script/profile digests, platform,
positive controls, restricted observations, remaining proof, and the explicit
result `engine_admission: not_established`. Exit zero means the offline canary
expectations matched, not that an engine is safe to admit. A dirty run is local
development evidence and cannot attest a clean release revision.

The regression suite tests permission-specific denials, broken profiles,
timeouts, crashes, malformed/truncated output, removal of restrictions, and
failed positive controls. macOS integration cases run actual children; they are
skipped on other operating systems, while evidence-classification tests remain
portable. An Apple Event policy-query result cannot be substituted for an
actual effect-denial result.

## Provider scope finding

The installed Claude CLI reports `2.1.231`; this is an observation, not an
admission pin. Claude documents shell-subprocess sandboxing separately from
file-tool permissions and other integration boundaries. Therefore toggling its
sandbox setting does not establish isolation for the complete worker process.
The C1 design now requires complete launch coverage and refuses an unrestricted
fallback. Source: [Claude sandbox scope](https://code.claude.com/docs/en/sandboxing#scope).

## Remaining work, in dependency order

1. **Whole-provider enforcement.** Build a production allowlist around the
   provider process, or prove an equivalent complete provider boundary. Bind
   the executable and effective settings; cover file tools, Bash, hooks, plugin
   code, MCP children, detached descendants, and inherited descriptors. Admit
   only the required inference destinations and worker tool transport. Test
   real provider startup and nested-sandbox compatibility without granting
   access to the Heiwa user credential or approval store.
2. **Authority and grants.** Keep user and worker channels separate. Authenticate
   the CLI principal, carry an expiring Work grant, revalidate it at the existing
   Rust service boundary, and test direct CLI/API attempts, stale/revoked grants,
   grant expansion, and decision-store access. A provider permission response
   must never mint user authority. Model output and MCP annotations cannot grant
   privileges to Heiwa's internal tool loop either.
3. **Work continuity.** Bind desktop and CLI submissions to validated thread
   membership and a durable Work. Preserve lightweight read-only Answer turns;
   multiple bounded reads are not by themselves an execution task.
4. **Real effect and recovery tests.** Use an explicitly disposable source-app
   fixture for Apple Events and Reminders integration. Test approval lifecycle,
   revoked access, and crash/reconcile behavior with no duplicate effects.
   Those tests are separate from this offline experiment.
5. **Admission integration.** Only a complete, version-bound matrix can admit
   Claude to C1. Invalidate evidence when executable/config/profile/host inputs
   change. Missing or unsupported boundaries stay unavailable. Finish C1-a's
   runtime tests before claiming the sub-plan complete; C1's full acceptance
   gate remains a later end-to-end requirement.

No provider configuration, permission setting, installed runtime, release, or
public branch was changed by this spike.
