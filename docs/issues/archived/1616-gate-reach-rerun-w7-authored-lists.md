---
id: 1616
title: "Gate re-run 2026-10-01, W7: 13 gates still check an authored list where the population should be harvested"
status: resolved
resolved_in: 2026-10-02
type: tech-debt
area: testing, build
severity: medium
found: 2026-10-01
related: [phase-472, 1614, 1615, 1616, 1617, 1618]
---

## What

The 2026-10-01 re-run of the phase-472 audit
([findings](../../development/audit-findings-2026-10-01-rerun.md)) re-applied each
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

## Resolution (2026-10-02)

Every population that was an authored list is now harvested. Each row's
recorded mutation now fails (rc 0 → 1), and all 14 controls still fail.

| gate | the harvested population |
| --- | --- |
| `cargo-dir-knob-key` | every `nros_shared_cargo_dir` / `nros_share_corrosion_cargo_dir` CALL SITE in `file_kinds cmake` must reach a var last assigned by `nros_knob_key_fields` (new `check-cargo-dir-knob-key-sites.py`, run from the gate). The wrapper and the probe are `harvest` exemptions with reasons. |
| `codegen-version-surface` | the Rust runtime crates are the `nros_*::` paths the templates name, resolved to tracked packages. `nros_rmw` joined (`register_type_descriptor`), and the baseline was re-recorded at the UNCHANGED version 8: the surface existed at 8, and only the gate's reach moved. `nros_rmw_cyclonedds` (named only in a template comment) is exempt. |
| `entity-slot-costs` | every `.rs` under `nros-node/src` (the executor's crate), comment-stripped, not the SPIN/ACTION pair. |
| `entry-session-name` | every entry-pack `.jinja` that emits a runner call, plus `run_tiers` and the jinja callee spelling. 3 → 7 calls checked. |
| `fixture-id-guard` | every `scripts/` shell file that takes an id filter (`--id)` / `${NROS_FIXTURE_ID`) must call `nros_fixture_id_no_match` in code. |
| `gate-cache-keys-agree` | the consumer direction too: a restore-only key that neither job writes fails. |
| `lane-coverage-labels` | a PR-coverage claim is a clause SHAPE (compile tier/full + PR), not three phrases. |
| `ledger-orphan-refs` | a prefixed citation that is neither repo-rooted nor our crate falls through to the bare check, unless a segment is an upstream include root. |
| `no-alloc-image` + `no-direct-kernel-alloc` | ONE shared definition, `scripts/lib/kernel_alloc.py`: each kernel's allocator naming scheme. The source gate strips comments with `comments.py` instead of dropping `#…` lines, which hid `#define X pvPortMalloc`. |
| `nuttx-links-snapshot` | every tracked Rust source (1326, with the `population` floor). `join("staging")` is allowed exactly once, in the resolver. |
| `rust-stdio-on-zephyr` | every crate whose package name says Zephyr (board + examples), plus nros-c/nros-cpp. `nros-zephyr-build` (host build helper) is exempt. |
| `staleness-probe-exemptions` | every non-test fn whose body calls `staleness::begin_probe()`, 4 probes. `require_prebuilt_row_binary_fresh` was missed by the name prefix. |
