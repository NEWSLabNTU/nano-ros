---
id: 1951
title: "codegen-version-refusal's freshness arm drove ANOTHER checkout's `nros`, so the fast tier was red in every pristine clone opened from an activated shell"
status: resolved
type: bug
area: [testing, ci, codegen]
severity: medium
found: 2026-10-18
related: [1850, 1280, 1373, 1360]
---

## Measured

`just ci gate` on a linked worktree (2026-10-18), step
`check::pristine-checkout` (issue 1373): `1 of 456 gate(s) FAILED` —
`codegen-version-refusal`, arm E, "the freshness probe could not configure".
Reproduced in the kept clone, at the branch tip and at `origin/main` alike:

```
nros codegen --ament-witness failed (exit 2) ...
    error: unexpected argument '--ament-witness' found
```

The clone had no CLI of its own, so `nros_cli_bin` fell through to PATH —
which the session had inherited from the MAIN checkout, whose `nros` predates
`--ament-witness`.

## Why

Arm E proves the build REGENERATES this tree's interfaces at this tree's
codegen version. It resolved its CLI with plain `nros_cli_bin` (rungs
`$NROS_CLI`, this checkout, PATH, store), so with no CLI built here it measured
a different tree's binary. Issue 1850 added `--checkout-only` for exactly this
— a gate precondition that must skip, not borrow — and this site was not
moved onto it. It is invisible on CI (no foreign checkout on PATH) and in a
worktree that ran `just setup-cli` (rung 2 wins), which is why only the
pristine clone sees it.

## Resolution (2026-10-18)

Arm E resolves `nros_cli_bin --checkout-only`, and with no CLI built here
records `NOT VERIFIED` through the `nros_check_skip` ledger with the remedy
(`just setup-cli`) instead of a bare SKIP line. Measured both shapes: the
pristine clone skips through the ledger and the gate passes; a worktree with
its CLI built runs arm E (`ok E the tree is REGENERATED at 18`).

Sweep: `git grep -n nros_cli_bin -- tests scripts/check* scripts/ci | grep -v
checkout-only`. The remaining fallback callers are recipes that RUN a CLI
rather than measure one (`c-msg-gen-tests.sh`, `dep-chain-check.sh`, the
scaffold/template copy-out checks, `check-tier-preconditions.sh`); the
pristine-checkout gate passes all of them, which is the evidence this rule
needs and a grep is not.
