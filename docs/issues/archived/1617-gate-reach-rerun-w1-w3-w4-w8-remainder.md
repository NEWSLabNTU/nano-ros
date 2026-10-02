---
id: 1617
title: "Gate re-run 2026-10-01: 10 holes in classes W1, W3, W4 and W8 — a composite action unread, comments and echoes counted, no population floor, exemptions wider than their reason"
status: resolved
resolved_in: 2026-10-02
type: tech-debt
area: testing, build
severity: medium
found: 2026-10-01
related: [phase-472, 1614, 1615, 1616, 1618, 1643]
---

## What

The 2026-10-01 re-run of the phase-472 audit
([findings](../../development/audit-findings-2026-10-01-rerun.md)) re-applied each
mutation the 2026-09-28 audit recorded, plus new audits and spot-checks. Every
gate below still exits **0** on the mutation in its row, and each has a positive
CONTROL that exits non-zero: the same defect placed where the gate does read.
So these are measured holes, not readings.

Class: **W1 / W3 / W4 / W8**.

| class | gate · facet | mutation | rc | control rc | source |
| --- | --- | --- | ---: | ---: | --- |
| W8 | `check-cmake-find-program-shadowed.py` | `cmake/NanoRosSdkPin.cmake` += set(_NROS_RERUN_PROG "no CACHE here") | 0 | 1 | recorded 2026-09-28 |
| W3 | `check-cmake-verb-reachable.py` | new `cmake/NanoRosRerunDead.cmake`; new `docs/development/rerun-mention.md` | 0 | 1 | recorded 2026-09-28 |
| W3 | `check-core-crates-are-no-std.py` · block-comment | `packages/core/nros-core/src/lib.rs`: /* | 0 | 1 | spot-check |
| W8 | `check-default-gates-run-somewhere.py` · if-false | `.github/workflows/gate.yml`: if: false | 0 | classifier: `_events_of('if: false')` credits every event | recorded 2026-09-28 |
| W4 | `check-goal-cdr-stripped.py` · no-floor | `packages/api/nros-c/src/action/client.rs`: pub extern "C" fn nros_action_client_send_goal_raw( | 0 | 1 | recorded 2026-09-28 |
| W3 | `check-interop-cell-runners.py` · echo | `just/native.just`: @echo "run binary(=advertised_state_interop) by hand" | 0 | 1 | recorded 2026-09-28 |
| W8 | `check-nextest-binary-filters.py` · other-workspace | `.config/nextest.toml`: filter = "binary(integration_tests)" | 0 | 1 | recorded 2026-09-28 |
| W3 | `check-no-vacuous-tests.py` · return-as-effect | `packages/testing/nros-tests/tests/qos.rs` += fn rerun_prints_and_returns() { | 0 | 1 | recorded 2026-09-28 |
| W8 | `check-rmw-slot-producers.py` · test-only-reader | `packages/rmw/zenoh/nros-rmw-zenoh/tests/zenoh_integration.rs` += fn rerun_reader(vtable: &nros_rmw_cffi::NrosRmwVtable) -> bool { | 1 | expected rc 0 (the slot stays inert); got 1 — a test-only reader made `feature_supported` count as reachable | recorded 2026-09-28 |
| W1 | `check-workflow-doctor-after-setup.py` · composite-action | `.github/actions/setup-nros-cli/action.yml` += run: bash scripts/ci/runner-doctor.sh | 0 | 1 | spot-check |

## Direction

W1 → `workflow_commands.ci_files(include_actions=True)`; W3 → `scripts/lib/comments.py`; W4 → `scripts/lib/population.py` floor; W8 → `scripts/lib/exemptions.py` keyed on the exact shape the reason covers.

Per CLAUDE.md "Fix the CLASS": move each gate onto the class's shared helper,
add the negative control its row names, and re-run the row's mutation to show
it now fails. Phase-472's acceptance ("no confirmed hole in any class") stays
unmet until this list is empty.

## Resolution (2026-10-02)

All ten rows now fail on their mutation. The scratch harness confirmed each
mutation applied (`git status` non-empty) and restored the tree after each run.

| gate | fix | mutation rc (old → new) |
| --- | --- | ---: |
| `cmake-find-program-shadowed` (W8) | The CACHE/PARENT_SCOPE exemption is keyed on the keyword in CODE (`comments` with `strings=True`), not a substring of the line. Population moved to `file_kinds` with the `population` floor. The selftest now runs on the normal path, so the script left the selftest baseline. | 0 → 1 |
| `cmake-verb-reachable` (W3) | A referrer must be code that `comments.py` models, with the name surviving comment stripping. A Markdown page or a comment no longer counts. | 0 → 1 (+ a commented `include` row, 0 → 1) |
| `core-crates-are-no-std` (W3) | Matches `#![no_std]` in comment-stripped Rust. | 0 → 1 |
| `default-gates-run-somewhere` (W8) | A constant-false guard (`if: false`, `${{ false }}`, a `false` conjunct with no `\|\|`) admits no event. Every gate has ≥2 placements, so the proof is at the classifier, as the original row's was. | classifier 0 → 1 |
| `goal-cdr-stripped` (W4) | Population is every `extern "C" fn` (`pub`/`unsafe` optional), comment-stripped, with a normal-path selftest. Mutation re-expressed: drop `unsafe` AND the strip in the same arm, because dropping `unsafe` alone is not a defect. | 0 → 1 |
| `interop-cell-runners` (W3) | `echo`/`printf` lines are prose. Runner files are comment-stripped by language. | 0 → 1 |
| `nextest-binary-filters` (W8) | Targets come from the ROOT workspace's members only. A `tests/integration_tests.rs` in `packages/cli` no longer satisfies a root filter. | 0 → 1 |
| `no-vacuous-tests` (W3) | A bare `return;` is control flow, not an effect. | 0 → 1 |
| `rmw-slot-producers` (W8) | A reader in `tests/`, in `packages/testing/` or in a `#[cfg(test)]` item is not a runtime consumer. This moved `publisher_count_matched_subscriptions` / `subscription_count_matched_publishers` to inert, and both are now the `matched-counts` family (`defer = 1643`) and `not-implemented` in `rmw-api-map.toml`: only tests read them. Filed as issue 1643. | the recorded rc 1 (stale-family) → 0, as the row expected |
| `workflow-doctor-after-setup` (W1) | Reads composite actions via `workflow_commands.load_workflows(include_actions=True)`. `just setup-cli` / `setup-launch-resolve` no longer count as `just setup`, which is what credited the action's later doctor step. | 0 → 1 |

Controls unchanged: `cmake-verb-reachable/no-doc`, `nextest-binary-filters/missing`
and `no-vacuous-tests/print-only` still fail (rc 1).
