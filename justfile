set shell := ["bash", "-cu"]

python := ".venv/bin/python"
pytest := ".venv/bin/python -m pytest"

default:
    @echo "Product graph recipes:"
    @echo "  test-hub       Run hub test suite"
    @echo "  test-trading   Run incubator trading tests"
    @echo "  check-web      Validate static web surface"
    @echo "  check-docs     Build MkDocs docs strictly"
    @echo "  test-product   Run product test recipes"
    @echo "  check-product  Run product verification recipes"
    @echo "  verify-product Run product tests and checks"
    @echo "  deploy-product Push main to trigger CI deploy"

test-hub:
    {{pytest}} apps/heiwa_hub/tests -q

test-trading:
    cd apps/heiwa_trading && PYTHONPATH=src ../../{{python}} -m pytest tests -q

check-web:
    {{python}} apps/heiwa_web/scripts/check_static_surface.py

check-docs:
    {{python}} -m mkdocs build --strict

test-product: test-hub test-trading

check-product: check-web check-docs

verify-product: test-product check-product

deploy-product:
    git push origin main
