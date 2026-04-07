# Doctrine Phase 1 — Verification Artifact Pack

Module: `heiwa_production_db` published to local SpacetimeDB (`127.0.0.1:3000`)
Database identity: `c20039637172ba1a262bed38b83e2ae697860bae0fd20fcc17c50190289f85fb`

## Build gates passed

```
cargo check --workspace          # clean, 7 pre-existing warnings
spacetime generate               # WASM build + 70 binding files regenerated
cargo check -p heiwa-bindings    # zero warnings
cargo check -p heiwa-stdb        # zero warnings
spacetime publish -s local       # module deployed to local instance
```

## Reducer calls executed (in order)

### Knowledge plane

```
1. upsert_decay_profile("dp-crypto", "crypto_markets", 3600, 0.3, "full_reset", "auto_schedule")
   → success

2. ingest_source("src-001", "web", "https://example.com/btc-analysis", "abc123hash", ...)
   → success

3. mark_source_parsed("src-001", "parsed")
   → success

4. create_page("page-001", "research", "Bitcoin Market Analysis", "bitcoin-market-analysis", ...)
   → success
```

### Belief lifecycle

```
5. extract_belief_candidate("belief-001", "Bitcoin hashrate and price show strong positive correlation in Q1 2026", "crypto_markets", ..., 0.6, "dp-crypto", ...)
   → success, status="candidate"

6. corroborate_belief("belief-001", "src-001", 0.7)
   → success, corroboration_count=1

7. ingest_source("src-002", "web", "https://example.com/btc-hash2", ...)
   → success

8. corroborate_belief("belief-001", "src-002", 0.8)
   → success, corroboration_count=2, auto-transition candidate→supported

9. promote_belief("belief-001")
   → success, transition supported→durable
```

### Belief state after promotion (SQL query)

```
 belief_id    | status    | confidence | corroboration_count | freshness_score | promoted_at
--------------+-----------+------------+---------------------+-----------------+------------------------
 "belief-001" | "durable" | 1          | 2                   | 1               | (some = "1775582378")
```

### Treasury lifecycle

```
10. create_treasury("treasury-claude", "provider_account", "anthropic", "claude-pro", "day", 1000, 500000, 500000, 500000, 0.3, "strict", 0.3)
    → success, health_state="healthy", health_score=1.0

11. reserve_budget("res-001", "treasury-claude", "mission-001", 10, 50000, 25000, "1775590000")
    → success

12. record_spend("treasury-claude", 5, 20000, 15000, 12000)
    → success

13. record_provider_failure("treasury-claude", "429")
    → success, failure_streak=1
```

### Treasury state after spend+failure (SQL query)

```
 treasury_id       | health_state | health_score | spend_so_far_requests | spend_so_far_cost_millicents | failure_streak | last_429_at
-------------------+--------------+--------------+-----------------------+-----------------------------+----------------+------------------------
 "treasury-claude" | "healthy"    | 0.876        | 5                     | 12000                       | 1              | (some = "1775582388")
```

### Reservation state (SQL query)

```
 reservation_id | status | decision
----------------+--------+----------
 "res-001"      | "held" | "allow"
```

## Enforcement tests (all fail-closed as expected)

### Contradiction invariant

```
14. contradict_belief("belief-001", BOTH challengers set)
    → REJECTED: "exactly one of challenger_belief_id or challenger_source_id must be set"

15. contradict_belief("belief-001", NEITHER challenger set)
    → REJECTED: same error

16. contradict_belief("belief-001", source challenger only, "contra-001", severity=0.4)
    → success, belief transitions durable→contested
```

### Belief state after contradiction (SQL query)

```
 belief_id    | status      | confidence         | contradiction_count | contradicting_evidence_weight
--------------+-------------+--------------------+---------------------+------------------------------
 "belief-001" | "contested" | 0.7894736842105263 | 1                   | 0.4
```

### Contradiction record (SQL query)

```
 contradiction_id | primary_belief_id | challenger_source_id | resolution_status | severity
------------------+-------------------+----------------------+-------------------+----------
 "contra-001"     | "belief-001"      | (some = "src-002")   | "open"            | 0.4
```

### Page uniqueness

```
17. create_page("page-002", "research", ..., slug="bitcoin-market-analysis")
    → REJECTED: "page with namespace='research' slug='bitcoin-market-analysis' already exists (page_id=page-001)"
```

### Enum validation

```
18. ingest_source("src-bad", "invalid_kind", ...)
    → REJECTED: "invalid source_kind: 'invalid_kind' (allowed: [\"web\", \"pdf\", \"repo_file\", \"conversation\", \"api\", \"note\", \"dataset\"])"
```

## Summary

| Lifecycle | States observed | Enforcement verified |
|---|---|---|
| Belief | candidate → supported → durable → contested | promotion thresholds, contradiction invariant |
| Treasury | healthy (score=1.0 → 0.876) | spend tracking, failure recording, health recomputation |
| Reservation | held, decision=allow | budget gating persisted as row |
| Page | created, uniqueness enforced | (namespace, slug) rejection |
| Source | created, parsed | enum validation fail-closed |
