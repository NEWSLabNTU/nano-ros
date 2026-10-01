---
id: 1617
title: "Gate re-run 2026-10-01: 10 holes in classes W1, W3, W4 and W8 — a composite action unread, comments and echoes counted, no population floor, exemptions wider than their reason"
status: open
type: tech-debt
area: testing, build
severity: medium
found: 2026-10-01
related: [phase-472, 1614, 1615, 1616, 1617, 1618]
---

## What

The 2026-10-01 re-run of the phase-472 audit
([findings](../development/audit-findings-2026-10-01-rerun.md)) re-applied each
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
