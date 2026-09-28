/**
 * @heiwa/system1 — a two-tier decision engine.
 *
 * System 1 is a single parallel fan of schema-constrained judgments with
 * calibrated confidence. System 2 is whatever expensive generative path the
 * host already has. The gate between them is the product.
 *
 * See `architecture.md` for the topology, the request lifecycle, and the
 * failure-mitigation table.
 */
export * from "./core/system1/index.ts";
export * from "./orchestrator/index.ts";
