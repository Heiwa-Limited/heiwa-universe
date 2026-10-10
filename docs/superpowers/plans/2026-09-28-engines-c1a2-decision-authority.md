# C1-a2: file-bound decision authority

Implemented foundation, 2026-10-10. Planes: Execution and Evidence.

This incorporates the useful core primitives from experimental commits
44c2ab54 and a74cb03e into the current integration spine. The old incomplete
command scaffold and implementation plan remain recoverable in the operator's
private consolidation archive; they are superseded by this bounded contract.

## Authority and records

An approval decision requires a typed operator Principal backed by the
owner-private runtime credential file. A CLI environment variable cannot replace
that file. An authenticated runtime request must have Operator authority and
its effective credential must match the file; an ordinary User JWT cannot mint
operator authority. Client labels are bounded, self-reported attribution.

The shared core reader refuses unsafe names, symlinked/nonprivate secret
storage, nonregular files and oversized input. Runtime HMAC primitives provide
canonical, domain-separated tags covering the immutable decision content.
Readers require a matching filename/content ID, an approved or denied outcome,
a supported integrity version and a valid tag. Changes to any covered value
invalidate the decision. The key never enters records or prompts.

A historical, forged, unreadable or untagged file is inspectable evidence. It
cannot suppress pending work, count as a verified decision or satisfy a DREX
approval waiter. A conflicting stored decision is never overwritten or rerun;
the operator must reconcile it. There is no first-use legacy snapshot that
turns arbitrary local files into approvals. Mail staging, HTTP summaries and
notification scans use the same verifier as the decision command.

## Explicit limit

The tag establishes possession of a private local credential. It is not a
third-party signature, protection against another process with the same user
identity, or whole-provider containment. Work-bound grants, restricted process
construction and demonstrated enforcement remain separate C1-a requirements.
Calendar and Reminders resource-write admission is not granted by this slice.

## Verification

Durable regressions cover environment-only authority, unsafe secret paths,
canonical tags, tampered/mismatched IDs and outcomes, historical files remaining
pending, conflicting-record replay refusal, DREX waiter forgery, and Mail
staging. A real hermetic HTTP fixture proves User JWT read access cannot decide,
an operator/file-key mismatch cannot decide, and the matching operator produces
a verifiable receipt. The fixture executes only temporary local-file effects.

The source is accepted only after those tests, frozen clean source gates,
independent review and its matching isolated development bundle pass. Public
release evidence and installed behavior are separate receipts.
