---
id: 185
title: "zephyr self-pkg M-F.3 shim bake half-missing (config_cmake=false) + cli_bringup zephyr/platformio lanes red"
status: resolved
type: bug
area: zephyr
related: [phase-287, issue-0164, issue-0182]
resolved_in: "2026-07-13 fresh-fixture verification + west-fixture tool-hash guard"
---

## Summary (as filed)

Four lanes red on the 2026-07-12 sweep: the two `zephyr_self_pkg` tests
(`config_h=true, config_cmake=false` — the shim bake half-missing),
`cli_bringup_zephyr_adapter_shim_boots_native_sim`, and
`cli_bringup_platformio_…_bringup_builds`. Suspects were the three same-week
phase-287 emitter commits.

## Root cause — no emitter regression; museum west fixtures (#182 class)

Code inspection first: `codegen-system`'s `emit_bake_tree` writes
`system_config.h` and `system_config.cmake` unconditionally through one call
site, and the zephyr shim (`nros_system_generate`) FATAL-ERRORs if either is
missing — no current code path can produce the half-bake. A fresh
`scripts/build/west-fixtures.sh` run bakes 2/2 self-pkg fixtures with BOTH
files, and **all four lanes pass** against fresh fixtures + the current CLI
(serialized nextest, 4/4). The three suspect phase-287 commits are innocent.

The Jul-12 half-bake came from the sweep's west fixtures themselves: their
`.compile-ok` stamp was **date-only** — no input signature, no tool identity —
so the sweep could consume a bake produced by whatever CLI/tree state a prior
partial build left behind (the observed `.h`-without-`.cmake` is consistent
with a sweep-era `codegen-system` dying between the two writes, which the
build tolerates for self-pkg rows: `west build … || true` with a bake-exists
stamp gate). Unbisectable after the fact; the class is the same museum-fixture
blindness as #182, on the west lane the #182 guards didn't cover.

## Fix — the #182 guard, west edition

- `scripts/build/west-fixtures.sh`: `.compile-ok` now stamps
  `tool:nros=<sha256 of packages/cli/target/release/nros>` next to the date
  (shared helper, both fixture families).
- `require_west_fixture` (nros-tests): compares the stamped hash against the
  current CLI binary (same `sha256sum` hasher) and fails LOUD
  ("West fixture <id> is STALE — built with a different `nros` CLI") instead
  of soft-passing; a date-only legacy stamp reads as stale (one rebuild
  refreshes every stamp).

Verified: 4/4 lanes green under hashed stamps; negative-tested (corrupted
`tool:nros=` → the test fails with the STALE message; restored → PASS).

## Out of scope

`zephyr::test_zephyr_workspace_entry_native_sim_e2e` (the issue's fifth
bullet) still times out — it consumes the zephyr-family workspace-entry
image, whose rebuild belongs to the #164/#181 remediation, not this shim
(the issue itself predicted that split). It stays tracked there.
