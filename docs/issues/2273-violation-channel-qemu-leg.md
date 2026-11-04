---
id: 2273
title: "The contract-violation channel's end-to-end test runs on native_sim only; its QEMU leg was never built"
status: open
type: test-gap
area: [testing, zephyr, embedded]
severity: low
found: 2026-11-05
related: [phase-474, phase-528, 1714, 1715]
---

**Home: [phase-528](../roadmap/phase-528-run-on-host-and-emulator.md)** — split out of phase-474 T4 when that phase was archived (2026-11-05): a QEMU cell for an existing workspace, which is the road phase-528 puts the harness on.

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
