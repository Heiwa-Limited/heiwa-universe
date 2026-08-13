# `life_social_v1` — social projection contract

Consumed by `GET /api/v1/life/social`. Read from
`$HEIWA_STATE_DIR/life/social.json`, defaulting to
`~/.heiwa/state/life/social.json`.

## Producer ownership

**The producer is owner-only and is not shipped in this repo.** The runtime has
no SQLite dependency and does not read any operator database directly; taking
one so a read model could be served would put schema knowledge in two places
and tie the runtime's release cycle to an external tool's schema.

What is product here is **the contract**, not the producer. Any process that
writes a conforming file satisfies this route. The reference producer on the
author's machine is `~/heiwa-ea/publish.py`, which is personal tooling and
deliberately untracked.

## The runtime does not trust the producer

The projection is parsed into a closed schema
(`serde(deny_unknown_fields)`) and **re-serialised from the validated struct**.
Nothing is passed through from the file. A field the schema does not name
cannot reach a consumer, whether the producer is buggy, outdated, or hostile.

An earlier implementation deserialised into `serde_json::Value`, re-emitted it,
and checked four forbidden key names on the way past. That is a denylist, and a
denylist cannot enforce metadata-only — `snippet`, `raw`, `preview`, or any
nested object passes straight through.

Atomic publication protects a reader from *partial* bytes. Schema validation
protects it from *complete but unsafe* bytes. Both are required at this
boundary.

## Rejection conditions

The route answers `available: false` with a `reason` — never a 5xx — when:

| Condition | Reason |
| --- | --- |
| file absent | `no published social projection` |
| not valid JSON | `projection rejected by schema at line L, column C` |
| valid JSON, wrong shape (scalar, array, null) | `projection rejected by schema at line L, column C` |
| any unknown field, top level or per contact | `projection rejected by schema at line L, column C` |
| unknown aggregate-count key | `projection rejected by schema at line L, column C` |
| unknown `bucket`, `class`, `status` or `error` value | `projection rejected by schema at line L, column C` |
| malformed `generated_at` or contact `last` timestamp | `projection rejected by schema at line L, column C` |
| contradictory reconnect status/error/due combination | `reconnect state is inconsistent` |
| `name` or `relation` longer than 120 chars | `… exceeds 120 chars; labels only` |
| `schema_version` ≠ `life_social_v1` | `unsupported schema_version …` |
| `policy` ≠ `metadata-only-no-message-text` | `refusing anything but …` |

**Rejection reasons never echo producer content.** serde embeds the offending
value in errors like ``unknown variant `…` ``, so reporting them verbatim made
the rejection itself a leak channel: a projection carrying message text in a
bad field had that text reflected into the response refusing it. Only the JSON
location is reported.

A stale but valid projection **is** served, with `age_days` so the caller can
judge it. Staleness is the caller's decision; malformedness is not.

## Schema

```jsonc
{
  "schema_version": "life_social_v1",
  "generated_at": "2026-08-11T09:00:00", // parsed local timestamp
  "window_days": 90,
  "counts": { "friend": 12, "family": 3 },   // bucket -> count
  "messages_total": 5189,
  "identified_pct": 99.4,
  "live_relationships": 20,
  "reconnect": {
    "status": "ok",        // ok | error
    "error": null,         // producer_unavailable | ranking_failed | data_unavailable
    "due": [ { "name": "…", "relation": "friend",
               "stale_days": 62, "score": 74.4 } ]
  },
  "contacts": [ {           // identified contacts ONLY; see below
    "name": "…", "relation": "friend",
    "bucket": "friend",     // friend|family|work|group|ended|service|acquaintance|unidentified|other
    "class": "reciprocal",  // reciprocal|one_sided_out|one_sided_in|broadcast|incidental|group|ended|service
    "sent": 456, "recv": 916, "total": 1372,
    "active_days": 78, "last": "2026-08-10T20:00:00", // parsed local timestamp
    "stale_days": 1
  } ],
  "policy": "metadata-only-no-message-text"
}
```

`reconnect.status` is explicit because an empty `due` list is ambiguous
otherwise: "the calculation failed" and "nobody is due" call for opposite
responses and must not share a representation. Valid combinations are
`ok + error:null` and `error + error:<code> + due:[]`.

`messages_total` counts messages for every eligible rollup, including contacts
omitted because their identity was unresolved. `identified_pct` is the share
of that same message total attributable to successfully resolved identities;
a classified service/group row is not "identified" merely because its bucket
is known. Dropping an unresolved row must not silently improve either aggregate.

## Producer requirements

- **Metadata only.** No message text, subjects, previews or snippets. The
  runtime enforces structure, closed category/count keys, timestamps, and
  reconnect invariants. It cannot prove human-defined labels mean what their
  field names claim.
- **Omit unidentified contacts entirely.** Count them in `counts`, do not list
  them. Publishing a hash of the handle is not a fix: eight unsalted hex
  characters over the space of phone numbers is pseudonymous, not anonymous —
  the space is small enough to enumerate, so the digest is a reversible pointer
  wearing a disguise. An unnamed contact's identity carries no analytical value
  here; the aggregate is the whole signal.
- **Failure as a code, never a message.** `reconnect.error` is a closed set.
  An exception string can carry a query, a row, or message text, and the
  consumer cannot tell.
- **Atomic and durable write.** `mkstemp` a UNIQUE name in the same directory,
  write, `flush`, `fsync`, then `rename(2)` and `fsync` the parent directory.
  A fixed `<out>.tmp` with
  `O_CREAT|O_TRUNC` is not concurrency-safe — two publishers interleave into
  one file — and the mode argument is **ignored when the path already exists**,
  so a leftover world-readable temp file from a previous run gets reused as-is.
- **Owner-private.** Create the file `0600` inside a `0700` directory, with
  those modes set at creation. Writing first and `chmod`-ing after leaves a
  window where real content is world-readable.

## What this schema cannot prove

It proves **which fields exist**, and for closed sets **which values they
hold**. It cannot prove that the string in `name` means a name. `name` and
`relation` carry operator-defined labels, so a producer writing message text
there satisfies every check here; length bounds shrink that channel without
closing it.

That residual is a producer contract, not a runtime guarantee. Stated here
rather than implied away.

## Tests

`apps/heiwa_shell/src/cmd/life.rs::social_projection_tests` — hermetic,
path-injected, covering valid, missing, malformed, bare-scalar, unknown field
(top level and per contact), closed aggregate keys, parsed timestamps, version
mismatch, policy mismatch, staleness, reconnect invariants, and
`HEIWA_STATE_DIR` resolution.

`api_payload_wires_life_social_route` in `app.rs` covers **route wiring only**.
It points `HEIWA_STATE_DIR` at an empty temp directory so it never reads the
operator's real projection, and holds the same env lock as the projection
tests — `std::env::set_var` mutates state shared by the whole test binary, and
cargo runs tests in parallel threads.
