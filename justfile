set shell := ["bash", "-cu"]

# Transitional product graph: Rust + TypeScript + Shell is the target stack.
# Python recipes remain here as regression coverage during migration.
python := ".venv/bin/python"
pytest := ".venv/bin/python -m pytest"

default:
    @echo "Product graph recipes:"
    @echo "  test-hub       Run legacy Python hub regression suite"
    @echo "  test-trading   Run incubator trading tests"
    @echo "  check-web      Validate transitional web surface"
    @echo "  check-docs     Build MkDocs docs strictly"
    @echo "  fmt-docs       Format authored markdown (root + docs/) via deno fmt"
    @echo "  check-fmt-docs Check markdown formatting without writing"
    @echo "  check-machine-security Inspect/fix owner-local machine security posture"
    @echo "  verify-security Run dependency/security/type/product-surface gate"
    @echo "  rotate-security Run weekly security rotation and write ~/.heiwa evidence"
    @echo "  test-product   Run product test recipes"
    @echo "  check-product  Run product verification recipes"
    @echo "  verify-product Run product tests and checks"
    @echo "  deploy-product Push main to trigger CI deploy"

test-hub:
    {{pytest}} apps/heiwa_hub/tests -q

test-trading:
    cd apps/heiwa_trading && PYTHONPATH=src ../../{{python}} -m pytest tests -q

check-web:
    {{python}} apps/heiwa_app/scripts/check_static_surface.py

check-docs:
    {{python}} -m mkdocs build --strict

# Scope lives in deno.json: authored docs only (root *.md + docs/); ops/, legacy/, .worktrees/ excluded
fmt-docs:
    deno fmt

check-fmt-docs:
    deno fmt --check

check-machine-security:
    bash scripts/check_machine_security.sh --fix

verify-security:
    bash scripts/verify_security.sh

rotate-security:
    bash scripts/weekly_security_rotate.sh

test-product: test-hub test-trading

check-product: check-web check-docs

verify-product: test-product check-product

deploy-product:
    git push origin main
