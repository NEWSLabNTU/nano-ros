---
id: 1881
title: "`check-lane-contracts` stayed green when a merge-gating job ran `just test-all` or `just test`: the gate could not parse variadic recipes, follow embedded `just` calls or read a precondition stated in a sourced shell function"
status: resolved
type: bug
severity: medium
area: [testing, build]
related: [phase-489, phase-396, 1030, 1656, 1226, 1882]
found: 2026-10-14
resolved_in: "PR #PRNUM"
---

## Measured

Found in phase-489 W2 and reproduced against `main`'s `gate.yml` (72ae36a9e4).
Each mutation below adds one step to `check-workspace-all`, guarded
`pull_request`/`merge_group`, and runs `python3 scripts/check-lane-contracts.py`.

| step added | before | after |
| --- | --- | --- |
| `just test-all` | OK | `[gating] gate.yml:check-workspace-all runs test-all -> _require-fixtures hard-requires just build-test-fixtures` |
| `just test tier1` | OK | same finding, via `test` |
| `just check build` | OK | OK (correct, see below) |
| `just check source-gates` | OK | OK (correct, see below) |

`main`'s workflows stay green after the fix. The rule resolves **83** CI lane
invocations now. It resolved **63** before, because 20 were invisible.

## Cause

There were four separate blind spots, and each one alone was enough to hide
`test-all`:

1. **Variadic recipes were not recipes.** `RECIPE` did not accept a `*`/`+`/`$`
   parameter, so `build *scope`, `test *scope`, `setup *ARGS`,
   `_nextest-tolerant +nextest_args` and others were missing from the
   recipe map. `just build tier2-nightly` and `just test` resolved to nothing.
2. **Only whole-line `just a b` calls were edges.** The user verbs call
   `just _test-scope "$tok" || rc=1`,
   `just _require-build-sources _require-fixtures-ready … || exit 1` and
   `nros_scope_exec just build-test-fixtures "lane=$tok"`, and `closure`
   followed none of them.
3. **A precondition one file away went unread.** `_require-fixtures` is
   `source scripts/build/fixture-lane.sh` plus `nros_fixtures_stamp_require`.
   The `Run:  just build-test-fixtures` / `return 1` pair is inside that
   function. `required_producers` read only recipe bodies. `test-all`'s
   `--workspace` run names no `--test`, so `tests_invoked` did not catch it
   either.
4. **The producer exemption was per JOB.** Any job that ran any producer
   skipped every rule. The compile-stamp rule for CI jobs still used the
   closure-wide "is the stamp builder run at all" presence test, which issue
   1656 had already replaced with a per-row check for the authored tiers.

## Fix

All in `scripts/check-lane-contracts.py`:

- `RECIPE` accepts `*`/`+`/`$` parameters.
- `embedded_just_calls` turns `just <recipe>…` anywhere on an executable line
  into an edge. Lines that are comments or `echo`/`printf` are skipped. A word
  counts while it is a recipe, and `<a>::<b>` counts as one edge when it exists.
- `sourced_function_text` reads the column-0 functions of every `source`d
  script that a body calls, transitively within that file, with comments
  stripped.
- `remedy_groups` / `_remedies_before_failure`: a producer is a precondition
  when the next `exit`/`return` after the line that names it is non-zero.
  Producers named on one remedy line are alternatives, as in `native::check`'s
  "generate-bindings (or build-test-fixtures)". The old "names it anywhere,
  exits non-zero anywhere" pairing would have read `build`'s closing advice
  (`Run "just build-examples"`, after an `exit 2`) as a hard requirement on 6
  jobs.
- `job_producers` works out what a job builds from the closure of the lanes it
  runs, so `just build tier2-nightly` counts as `build-test-fixtures`. The
  per-job exemption is now per producer. The CI compile-stamp rule asks
  `missing_stamp_rows`, the per-row check shared with the authored tiers.
- The CI-job loop moved into `ci_job_findings()` so the selftest can run it on
  a synthetic workflow. There are eight new selftest cases: both mutations,
  producer credit through a quoted call, echoed advice, alternatives, a comment
  in the sourced file, and two per-row cases. Reverting the parser fix, the
  sourced-function reader or the embedded-call edge each turns at least one
  case red.

## Why `just check build` / `just check source-gates` stay green

Neither one is a violation of the rule this gate enforces. `check::source-gates`
builds every stamp row its three tests assert in its own body
(`NROS_COMPILE_CHECK_LANES=cxx-syntax,cxx-syntax-verdict,cxx-compile-verdict`),
and the per-row check confirms that for all 13 rows. `check::build` builds
`borrowed-e2e`'s stamp the same way. The one hard requirement `check build` used
to carry was `native::check`, which needs `generate-bindings`. That recipe is no
longer reached at all. Issue 1882 has the details.

When `@just native check` is put back into `check::build` on a scratch tree,
the fixed gate reports
`check-workspace-all runs check::build -> native::check hard-requires just
build-test-fixtures or just generate-bindings`. So the reach is in place for
when that coverage comes back.

No real violation surfaced on `main`.
