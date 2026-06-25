---
id: 102
title: "~60 examples ship but are never built/tested as fixtures; advanced capabilities are native-only"
status: open
type: tech-debt
area: testing
related: [rfc-0026, phase-263]
---

## Summary

A 2026-06-26 audit cross-checking `examples/README.md` (the RFC-0026 coverage matrix) against
the real tree + `examples/fixtures.toml` + `packages/testing/nros-tests/tests/` found ~60
example projects that exist (and are claimed in the matrix) but are **never built+tested as
fixtures** — they are documentation-only and unverified. Separately, every advanced runtime
capability is exercised on `native` only.

## Holes (priority order)

**P0 — claimed in matrix, zero single-node fixtures:**
- **Zephyr: 22 examples, 0 single-node fixtures** (`examples/zephyr/{c,cpp,rust}/` — talker,
  listener, service-{server,client}, action-{server,client}, +typed/async/cyclonedds). Only
  one workspace fixture (`workspace-rust-zephyr`, `skip_probe=true`). `phase_118_collapse.rs`
  builds some dynamically, outside the fixture manifest.
- **FreeRTOS C/C++: 12 examples, 0 single-node fixtures** (`examples/qemu-arm-freertos/{c,cpp}/`).
  Rust has 7 fixtures. (NB: workspace-entry e2e for C/C++ FreeRTOS is landing under phase-263
  C2x — `c_freertos_entry_e2e.rs`, `cpp_freertos_entry_e2e.rs` — but the single-node examples
  remain untested.)
- **NuttX C/C++: 12 examples, 0 single-node fixtures** (`examples/qemu-arm-nuttx/{c,cpp}/`).

**P1 — partial:**
- threadx-riscv64 cyclonedds C/C++: only talker+listener fixtures; service/action examples
  exist but have no fixtures.
- native C/C++ variant examples (custom-msg, custom-transport-loopback, logging,
  component-poc, transform-poc): exist, 0 fixtures.
- native Rust async (`action-client-async`, `service-client-async`): exist, 0 fixtures.

**P2 — capabilities native-only** (no embedded fixture exercises them): lifecycle, parameters,
safety/CRC, QoS-overrides, RT-tiers, multihost. Each has exactly one native fixture and no
cross-platform coverage.

**P2 — stale examples to delete (exist but dropped/broken):**
- `examples/zephyr/rust/service-client-async` — dropped from the matrix (README line ~81) but
  the dir was never removed.
- `examples/stm32f4/rust/talker-embassy` — `skip_build=true` (missing platform glue), tracked
  in known-issues; fix-or-remove.
- `examples/px4/rust/uorb` — README-only placeholder (Rust crate deleted Phase 115.K.4; C++ is
  canonical).

## RMW reach

zenoh: broad. cyclonedds: native + threadx only (zephyr examples exist but untested). xrce:
native + one baremetal. uORB: px4 C++ stub only.

## Fix direction

Per hole: either (a) add the fixture rows to `examples/fixtures.toml` + a behavior test under
`nros-tests/tests/`, or (b) honestly de-scope the cell from the RFC-0026 matrix so the matrix
stops over-claiming. Do NOT leave examples in-tree that the matrix lists but CI never builds —
that reads as "covered" when it isn't (the project's "no silent caps" principle). The
`example_shape.rs` / `examples_canonical_shape.rs` tests already assert structure; extend the
gating so a matrix cell without a fixture is a tracked exception, not a silent gap.

Sequence: P0 zephyr single-node fixtures (biggest claim/reality gap) → P1 freertos/nuttx
single-node C/C++ + threadx cyclone svc/action → P2 capability-on-embedded + stale cleanup.

## Evidence

2026-06-26 coverage audit; full platform×lang×capability grid + the ~60-example untested list
in that audit. Snapshot caveat: phase-263 C2x is actively landing C/C++ embedded **workspace-
entry** e2e tests, which narrows the workspace axis but not the single-node-example holes.
