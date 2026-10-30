---
id: 2158
title: "Three fixture comments said a live fixture was built or consumed by nothing"
status: resolved
resolved: 2026-10-30
type: bug
area: [testing, docs]
severity: low
found: 2026-10-30
related: [phase-515]
---

## What

The phase-515 W0.d prose pass (#2855) rewrote three comments to say the
code they describe had no builder or no consumer:

- `packages/testing/nros-tests/fixtures/diagnostic_rustc_fixture/src/lib.rs`
- `packages/testing/nros-tests/fixtures/multi_pkg_workspace_freertos/src/talker_pkg/src/lib.rs`
- `packages/testing/nros-tests/src/fixtures/binaries/mod.rs`
  (`build_nuttx_riscv_workspace_cpp_realtime_entry`)

All three are live:

- `diagnostic_rustc_fixture` is the `diagnostic_rustc_verbatim` fixture row,
  run by `cargo check`, and `diagnostic_verbatim::rustc_diagnostic_verbatim`
  greps its stderr.
- `multi_pkg_workspace_freertos` is cross-built by the `freertos_firmware`
  row; `freertos_firmware_entry` checks its generated `run_plan.rs`.
- the riscv C++ realtime entry is consumed by `realtime_tiers_e2e` (once
  ~~`realtime_tiers_cpp_riscv_nuttx_e2e`~~).

## Cause

The lookup that produced the claims passed several paths to `git grep` as ONE
quoted argument. That names a path that does not exist, so the search
matched nothing and read as "no reference". A comment written from an empty
search states a falsehood with the confidence of a measurement.

## Resolution (2026-10-30)

The three comments now name their row and their test. The other absence
claims made in the same pass (CYCLONEDDS_PRESENT, the dds type-name pin, the
pool-exhaustion marker, the tier roundtrip, `announced_names`) were re-checked
with one path per argument and hold.

Every directory under `packages/testing/nros-tests/fixtures/` is referenced
outside `docs/`, so there is no unbuilt-fixture class to gate here; the
defect was the lookup, not the tree.
