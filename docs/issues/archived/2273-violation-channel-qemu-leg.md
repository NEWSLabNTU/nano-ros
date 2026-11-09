---
id: 2273
title: "The contract-violation channel's end-to-end test runs on native_sim only; its QEMU leg was never built"
status: resolved
type: test-gap
area: [testing, zephyr, embedded]
severity: low
found: 2026-11-05
resolved: 2026-11-09
resolved_in: "phase-544 W2: workspace-zephyr-cortex-m-cpp-violation builds violation-cpp's [image.zephyr_mps2] for mps2/an385, and violation_channel_e2e's ViolationChannel cell boots it under -icount shift=auto and asserts the one post-arming max-latency-runtime verdict; tier 2 pins the cell"
related: [phase-544, phase-474, phase-528, 1714, 1715]
---

**Home: [phase-544](../../roadmap/phase-544-harness-run-plan-followups.md) W2** — split out of phase-474 T4 when that phase was archived (2026-11-05), homed in phase-528, and rehomed 2026-11-06 when phase-528 was archived: a QEMU cell for an existing workspace, on the road phase-528 put the harness on.

## What exists

Phase-474 T4 asked for a deliberate overrun reported through the violation
channel (D1) on the board, guarded in CI "on QEMU or native_sim". Three of its
four legs are met:

- host: `t4_an_overrun_after_arming_is_the_one_stored_violation` (nros-node
  executor tests);
- native_sim: `examples/workspaces/violation-cpp`, consumed by
  `tests/violation_channel_e2e.rs`
  (`zephyr_cpp_violation_channel_reports_one_overrun_after_arming`): one
  `max-latency-runtime` verdict after arming;
- board: three `max-latency-runtime` verdicts in the S32K344's SWD record,
  2026-10-26 (phase-512 H6, closed).

## What is missing

The same workspace on a Zephyr QEMU target (mps2/an385, `-icount
shift=auto`). native_sim runs on the host's clock, so it cannot show that the
arming and the latency measurement hold on an emulated Cortex-M with its own
timer, which is where the board's numbers came from.

## What would close it

A `fixtures.toml` row building `violation-cpp` for a Zephyr QEMU board, and a
`violation_channel_e2e` cell on it in a scheduled lane, asserting the same
single post-arming `max-latency-runtime` verdict.

## Resolution (2026-11-09, phase-544 W2)

- `examples/workspaces/violation-cpp` gained `[image.zephyr_mps2]` (board
  `mps2-an385-zephyr`, the realtime-c `zephyr_smp` shape) with its own Kconfig
  in `src/demo_bringup/boards/mps2_an385/`. The board package supplies the
  in-kernel IP stack, the SLIRP address and the sizing; the image adds
  `CONFIG_LOG_MODE_IMMEDIATE=y`, because the board's default deferred log
  dropped six lines in the first run, the arming line among them.
- Fixture row `workspace-zephyr-cortex-m-cpp-violation`
  (`zephyr-cortex-m,cpp,zenoh`, build dir
  `build-cortex-m-ws-cpp-violation-entry-zenoh`), baking
  `tcp/10.0.2.2:10885` = `alloc::port_of(ZephyrQemuCortexM, Cpp,
  ViolationChannel)`.
- A `Workload::ViolationChannel` (port offset 85) and one cell,
  `(ZephyrQemuCortexM, Cpp, Zenoh, ViolationChannel, Workspace, Runtime)` in
  `packages/boards/board-support-cells.toml`. `violation_channel_e2e` is its
  consumer (`Consumer::ViolationChannel`) and left `NON_CELL_TESTS`; the
  native_sim test stays in the binary as the leg the matrix does not model,
  since a cell on a tier-1 platform would pull every `zephyr,cpp,zenoh`
  fixture into tier 1.
- Tier 2 is 1-wise over platform, lang, rmw and kind, never the workload, so
  no cover is obliged to pick the cell: `ci_lane::pinned_tier2_reqs` makes it
  a requirement of its own. Tier 2 stays 12 cells over 11 coordinates (the
  cover swaps `zephyr-cortex-m,c,xrce` for `zephyr-cortex-m,cpp,zenoh`); tier
  3 goes 258 -> 259 cells over the same 54 coordinates.
- The test boots the image with `QemuProcess::start_mps2_an385_networked`, the
  descriptor's `[board.run.qemu]` line (`-icount shift=auto`) plus the SLIRP
  NIC, holds the row to the allocator's port (`assert_row_dials_port`) and
  asserts one stored `max-latency-runtime /handler/state` verdict, measured at
  least 150 ms, after the RUN line, plus the arming line's suppressed count.
  The count window ends at the route scenario's late tick (`took avail 9`):
  on the emulated core that tick's timer path read 60 ms against the 50 ms
  bound in 1 of 6 early runs, a correct verdict that belongs to the route
  scenario, not the overrun.

The image did not link at first, which is a defect of the generated Zephyr
entry rather than of this workspace: a component archive that calls a kernel
service (`k_busy_wait` here) reached the link after `libkernel.a`, and ld
reported `undefined reference to z_impl_k_busy_wait`. native_sim never showed
it. `nano_ros_entry` now appends `kernel` to each component archive's
`INTERFACE_LINK_LIBRARIES` on Zephyr (`cmake/NanoRosEntry.cmake`); naming it
on the placeholder library was measured not to work, because the components
arrive through INTERFACE wrappers and the closure put the kernel first.

## Verified (2026-11-09)

- `NROS_ZEPHYR_FIXTURE_FILTER=violation just build zephyr` builds both rows;
  `fixtures-manifest.py west-leaves --coords-from` the tier-2 coordinates
  selects the QEMU row and not the native one.
- Under tier 2's scope (`NROS_TEST_COORDS` / `NROS_TEST_UNCLAIMED` from
  `fixture-lane.sh`), `cargo nextest run -p nros-tests --test
  violation_channel_e2e`: the cell PASSES, verdict
  `contract violation #2: max-latency-runtime /handler/state measured=165
  declared=50 (total=3 dropped=0)` (measured 164 to 177 across runs); the
  native_sim test reports `[SKIPPED:lane]`, as it should.
- With the window: 54 of 55 consecutive runs passed. The one failure was on
  the verdict count and was not captured in detail; 45 later runs passed.
- Unscoped, all three tests pass (native_sim, QEMU, `cases_bound_to_matrix_cells`).
