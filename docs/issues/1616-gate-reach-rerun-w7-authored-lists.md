---
id: 1616
title: "Gate re-run 2026-10-01, W7: 13 gates still check an authored list where the population should be harvested"
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

Class: **W7 — authored lists where the population should be harvested**.

| class | gate · facet | mutation | rc | control rc | source |
| --- | --- | --- | ---: | ---: | --- |
| W7 | `check-cargo-dir-knob-key.sh` | `packages/api/nros-c/cmake/nros-nuttx.cmake`: set(_nnbe_knob_fields "") | 0 | 1 | recorded 2026-09-28 |
| W7 | `check-codegen-version-surface.py` · nros-rmw | `packages/core/nros-rmw/src/type_descriptor.rs`: _rerun: u8, | 0 | 1 | recorded 2026-09-28 |
| W7 | `check-entity-slot-costs.py` · other-file | `packages/core/nros-node/src/executor/mod.rs` += impl<'s> Executor<'s> { | 0 | 1 | recorded 2026-09-28 |
| W7 | `check-entry-session-name.py` · c-jinja | `packages/cli/nros-cli-core/src/codegen/entry/packs/entry/c/boot_wrapper.jinja`: ("", nros_boot_config_namespace; `packages/cli/nros-cli-core/src/codegen/entry/packs/entry/c/boot_wrapper.jinja`: (uint8_t)NROS_ENTRY_DOMAIN_ID, "", | 0 | 1 | recorded 2026-09-28 |
| W7 | `check-fixture-id-guard.sh` · workspace-builder | `scripts/build/workspace-fixtures-build.sh`: exit 0 | 0 | 1 | recorded 2026-09-28 |
| W7 | `check-gate-cache-keys-agree.py` · consumer-direction | `.github/workflows/gate.yml`: - name: rerun extra restore | 0 | 1 | recorded 2026-09-28 |
| W7 | `check-lane-coverage-labels.py` · phrase | `.github/workflows/gate.yml`: name: check (fast + full compile tier for every PR) | 0 | 1 | recorded 2026-09-28 |
| W7 | `check-ledger-orphan-refs.py` · crate-relative | `docs/reference/api-parity-ledger/metadata.json`: "why": "see `src/rerun_missing.rs` -- | 0 | 1 | recorded 2026-09-28 |
| W7 | `check-no-alloc-image.py` · k_realloc | new `tmp/rerun/ka.marker` | 0 | — | recorded 2026-09-28 |
| W7 | `check-no-direct-kernel-alloc.sh` · define | `packages/boards/nros-board-freertos/c/freertos_run_tiers.c` += #define RERUN_ALLOC(n) pvPortMalloc(n) | 0 | 1, 1 | recorded 2026-09-28 |
| W7 | `check-no-direct-kernel-alloc.sh` · k_calloc | `packages/boards/nros-board-zephyr/c/zephyr_run_tiers.c` += void *rerun_kc(void) { return k_calloc(1, 8); } | 0 | 1, 1 | recorded 2026-09-28 |
| W7 | `check-nuttx-links-snapshot.sh` | `packages/boards/nros-board-nuttx-qemu/build.rs` += fn rerun_live(p: &std::path::Path) -> std::path::PathBuf { | 0 | 1 | recorded 2026-09-28 |
| W7 | `check-rust-stdio-on-zephyr.py` · board-crate | `packages/boards/nros-board-zephyr/src/lib.rs` += fn rerun_print() { | 0 | 1 | spot-check |
| W7 | `check-staleness-probe-exemptions.sh` · row-probe | `packages/testing/nros-tests/src/fixtures/binaries/mod.rs`: (delete) staleness::record_fresh(&resolved).map_err(TestError::BuildFa | 0 | 1 | recorded 2026-09-28 |

## Direction

Harvest the population (`scripts/lib/harvest.py`, which also refuses a stale or reason-less exemption) instead of the authored list each gate carries.

Per CLAUDE.md "Fix the CLASS": move each gate onto the class's shared helper,
add the negative control its row names, and re-run the row's mutation to show
it now fails. Phase-472's acceptance ("no confirmed hole in any class") stays
unmet until this list is empty.
