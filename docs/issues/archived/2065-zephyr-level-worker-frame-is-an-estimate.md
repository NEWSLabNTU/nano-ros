---
id: 2065
title: "The Zephyr level worker frame is an unmeasured 4,096 B estimate"
status: resolved
type: tech-debt
area: [zephyr, executor, memory]
severity: low
found: 2026-10-26
resolved: 2026-10-26
resolved_in: "PR #2682"
related: [phase-509, rfc-0108, 2019]
---

## Measured

Issue 2019 measured a spawned level's worker frame on FreeRTOS (C 10,240 B,
Rust 48,128 B) and NuttX (C 11,264 B, Rust 32,768 B). Zephyr still used the
4,096 B estimate, `CONFIG_NROS_ZEPHYR_LEVEL_WORKER_FRAME`, for both runners.
A Zephyr level whose stack derives from member `stack_bytes` adds this frame
to the largest member, so the derived stack could be too small, and nothing
would report it.

## Resolution

Both runners are now measured with issue 2019's method: the high-water at
the first pass (the setup peak), then the steady peak over the passes. The
frame is the larger of the two, rounded up to the next KiB.

| Runner | Fixture | Setup peak | Steady peak | Frame |
|--------|---------|-----------:|------------:|------:|
| C | S32K344 island, 2 spawned levels (hardware) | 12,464 B | 6,920 B | 13,312 B |
| Rust | realtime-rust `derived_bringup`, mps2/an385 (QEMU) | 50,968 B | 5,952 B | 51,200 B |

- **C runner.** The island's lock probe ran on a normal act and on a 300 ms
  overrun act. At each level's first pass it read the thread analyzer's
  high-water (setup 12,328-12,464 B). It then repainted the stack below its
  own frame, so later scans show the passes alone (steady 5,584-6,920 B
  over 3,463-4,777 passes: spin_once and the C++ member callbacks).
- **Rust runner.** The same readout ran as a temporary patch to
  `entry_tiers.rs`; it is not merged. Setup was 50,968 B with
  `CONFIG_DEBUG` (the fixtures' profile) and 47,456 B in release. Steady was
  5,952 B and 5,816 B after 300, 3,000 and 10,000 passes.

No pass went deeper than setup. Each peak includes the fixture's own
registration, so it is an upper bound for that fixture.

- **C frame.** `CONFIG_NROS_ZEPHYR_LEVEL_WORKER_FRAME` now defaults to
  13,312 B, and the header fallback matches. The help says the Rust runner
  does not read the knob.
- **Rust frame.** `level_worker_frame_bytes("zephyr", Rust)` is 51,200 B.
- **Agreement checks.** `nros-mem-report.py`'s Zephyr default matches the
  Kconfig, and its selftest and the orchestration-ir C-frame test both read
  the Kconfig default back.
- **Per-level default.** `NROS_ZEPHYR_TIER_STACK_SIZE` stays 16,384 B, so a
  level whose members state nothing keeps 16 KiB. Only the per-callback
  default shrinks, to 3,072 B.
