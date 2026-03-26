# Ops Boundary

`ops/` contains operator-only context, archived dependency trees, and non-product working material.

Rules:

- `ops/` is outside the product build/test/deploy graph.
- `ops/` may read from product code and product docs.
- Product code, product tests, and deploy surfaces must not depend on `ops/` code.
- Root compatibility shims may point operators at canonical files under `ops/context/`, but product runtime behavior should come from `apps/`, `packages/`, `config/`, and `infra/`.
