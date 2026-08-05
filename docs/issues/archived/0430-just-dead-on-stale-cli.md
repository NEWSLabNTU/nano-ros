---
id: 430
title: A stale in-tree `nros` makes EVERY `just` recipe fail, including the one
  that rebuilds it
status: resolved
type: bug
area: build
related: [0363, 0197, phase-336]
resolved_in: 20028b818 + _require-profile-dir class-fix
---

`just` evaluates every justfile variable before running any recipe;
`qemu-baremetal.just`'s `PROFILE_DIR := \`… nros_cargo_target_profile_dir\``
called `nros profile` (phase-336) at PARSE time, so an `nros` predating that verb
bricked every recipe — including `just setup-cli`, the one that rebuilds it.

RESOLVED in two parts:
- **Brick (20028b818):** the backtick gained a trailing `2>/dev/null || true`, so a
  stale CLI yields an EMPTY `PROFILE_DIR` instead of a failed parse. Verified: with
  a failing `nros` on PATH, `just --list` / `just setup-cli` run normally.
- **Actionable-message coverage (this fix):** `_require-profile-dir` refuses an
  empty `PROFILE_DIR` with "the nros CLI is missing or predates `nros profile` —
  Run: just setup-cli", but only recipes that dep'd it got that message. `test-wcet`,
  `test-lan9118`, and `_run-qemu-mps2` (hence its talker/listener/rtic-* callers)
  interpolated `FIXTURE_TARGET`/`PROFILE_DIR` WITHOUT the guard, so a stale CLI gave
  a confusing "kernel not found". Added the guard dep to all three (0196 class-fix).

Direction (2) from the filing (move the query out of `:=` to recipe-local) is
INFEASIBLE: `FIXTURE_TARGET` is interpolated into recipe-ARGUMENT positions
(`_run-qemu-mps2 (FIXTURE_TARGET / "…")`), which resolve at parse time, so it must
stay a `:=` variable. The non-fatal `|| true` + the guard (loud at USE, silent at
parse) is the correct shape given that constraint.
