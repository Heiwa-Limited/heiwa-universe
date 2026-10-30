# Packaged CLI workflows

Plane: Intake and Evidence. These workflows compose existing local services;
they do not add a second connector, model route, approval authority, or script
interpreter.

```sh
heiwa workflows list
heiwa workflows show daily-brief --json
heiwa workflows run daily-brief --limit 10 --json
heiwa workflows run native-audit --json
heiwa workflows run runtime-check --json
```

`daily-brief` replaces repeated Calendar, Mail, and approval reads with one
compact response. It includes today's synced events, priority Mail headers,
pending approvals, and Reminders enrollment status. It uses local snapshots;
it does not refresh native apps or read Reminders content. Calendar sync status
and Mail snapshot presence are included so an empty or stale snapshot is not
mistaken for a live native read. Refresh through the existing connectors when
needed. Reminders content remains available through `heiwa reminders read`.

`native-audit` reports implemented Apple integration boundaries and Calendar /
Reminders enrollment. Enrollment is distinct from current macOS permission;
the audit neither probes TCC nor requests access. `runtime-check` reports the
current CLI path, version, app TCP reachability on 7474, and pending approval
count. TCP reachability alone does not prove listener identity or API health.

All three are deterministic and make no model calls, launch no scripts, prompt
for no permissions, and perform no external writes. `--json` uses the existing
`heiwa.cli/v1` envelope; run results carry `heiwa.workflows/v1`. List and show
expose each workflow's sources and freshness behavior. Unknown workflows and
flags fail before accessing sources. No user-supplied command is evaluated.

The daily brief defaults to ten rows per collection; `--limit` accepts 1–50.
Collections report available counts and truncation. Free text is capped at
240 characters per field and only declared scalar fields are returned. Large
source payloads, mail bodies, nested objects, and other calendar days are
excluded. This is a bounded read interface, not a lossless export.

New workflows should package a demonstrated repeated task and reuse its owning
services. Add inputs, declared effects, approval and retry rules, and receipts
before admitting any workflow that writes. An arbitrary shell runner is not
an appropriate extension of this read-only catalog.

## Desktop application location

The public macOS installer places the native bundle at
`~/Applications/Heiwa.app`. The compatibility path
`<runtime-root>/app/Heiwa.app` is a symlink to that one bundle. Bootstrap
preserves its native executable; checkout/dev bundle updates preserve the
alias and replace the canonical destination. A legacy internal launcher is
retained under `.Heiwa-before-canonical.*` during public installation.
Existing system-wide apps are not silently deleted by the installer.
