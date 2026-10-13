---
id: 1874
title: "Tier 2 builds only the C rows of the shared bare-metal group, but its run requires every binary in it, so 15 Rust and C++ cells fail as 'MISSING for an in-lane coordinate'"
status: resolved
type: bug
severity: high
area: [testing, ci]
related: [1684, 0828, 0584]
found: 2026-10-13
---

## Measured

`just ci matrix` (tier 2, after `just build-test-fixtures lane=tier2`): 15 of
its 21 real failures are bare-metal QEMU cells. Each read:

```
Test fixture binary MISSING for an in-lane coordinate:
  build/cargo-fixtures/baremetal/thumbv7m-none-eabi/nros-relwithdebinfo/qemu-rtic-talker
```

The failing cells include `test_qemu_rtic_*`, `test_qemu_baremetal_capi_e2e`'s
three C++ cases, `test_qemu_bsp_pubsub_e2e`, `test_qemu_lan9118_driver`,
`test_qemu_wcet_benchmark` and `logging_smoke_qemu_baremetal_mps2_…`.

Tier 2's coordinates select bare-metal once: `baremetal,c,zenoh`. The QEMU
family's build log shows the lane building exactly the six
`examples/mps2-an385-baremetal/c/*` rows, which is correct for that
coordinate.

## Cause

The build and the run narrowed by different rules:

- **The build** (`fixtures-manifest.py --coords-from`) narrows per ROW
  coordinate. The Rust rows (`baremetal,rust,zenoh`) and the C++ rows
  (`baremetal,cpp,zenoh`) are not tier-2 coordinates, so it does not build
  them.
- **The run** resolves a bare-metal binary through
  `require_shared_fixture_binary`. Its artifacts share one group dir, so a
  PATH attributes to no row. Issue 1684 therefore made it skip only when the
  lane selects NO coordinate on the PLATFORM. Tier 2 selects one, so every
  binary in the dir fell through to the presence check. The out-of-lane
  ones are absent by design, and the absence was then reported as a broken
  promise.

`every_unskippable_row_is_in_its_lane_build` passed throughout: the gate
modelled these rows as skippable, and the runtime check could not skip them.

## Fix

A shared-dir binary is attributed to its row by NAME:
`lane::rows_producing_binary` matches `[package] name` or a `[[bin]] name` in
each row's `Cargo.toml`. Exactly one producing row skips by that row's
coordinate. None or several keeps failing closed, as issue 1684 requires for an
ambiguous match.

Every bare-metal row builds exactly one binary, named uniquely (checked over
all 30 rows).

Test: `a_shared_group_binary_skips_by_its_own_rows_coordinate`, beside issue
1684's `a_shared_group_skips_only_when_its_platform_is_out_of_lane`, which
still passes.
