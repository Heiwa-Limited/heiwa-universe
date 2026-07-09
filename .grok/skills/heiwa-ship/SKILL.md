---
name: heiwa-ship
description: Commit, PR, and merge Heiwa changes safely with tests and plane-aware commit messages. Use when ready to ship a branch or the user says merge / ship / PR.
argument-hint: "[optional commit focus]"
user-invocable: true
---

# /heiwa-ship — land the work

1. `git status` / `git diff` — no secrets.
2. Run targeted tests for touched crates.
3. Stage only intentional paths (not junk, not `.env`).
4. Commit message: complete sentences, why + plane if useful.
5. Push branch; open PR; merge when green (or user-approved).
6. Never force-push `main`; never `--no-verify` unless user insists after failed hook diagnosis.

$ARGUMENTS
