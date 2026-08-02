---
id: 0387
title: C-arm (and partly C++) runtime lanes fail across platforms while the
  Rust arms pass — confirmed on fresh fixtures at low load
status: open
severity: high
created: 2026-08-02
tags: [c-api, cpp-api, threadx, zephyr, realtime, params]
related: [0380, 0356]
phases: [phase-326, phase-327]
---

## Evidence quality

Every failure below reproduced 3/3 on FRESHLY REBUILT fixtures (post
issue-0380 model restoration, CLI current, full family rebuild) at
`--test-threads 2` on an otherwise idle host — the stale-fixture and
QEMU-load explanations are eliminated. The same run's Rust siblings pass,
which rules out router/agent/env problems.

## The confirmed-red set (2026-08-02)

**Language pattern — C fails where Rust passes on the SAME platform:**

- `rtos_e2e` ThreadxLinux **C** pubsub (0 messages), **C** action
  (accepted=false), ThreadxRiscv64 **C** pubsub + service (0 responses) —
  Rust and Cpp arms of the identical cells PASS.
- `zephyr_edf_deadline_applied_c` — 0 `nros: EDF deadline set tier=` lines;
  the `_cpp` and `_rust` arms of the same fixture family PASS.
- `realtime_tiers_e2e` case_02_native_c, case_03_native_cpp,
  case_04_native_cpp_rclcpp — low tier never scheduled (`/telem` 0
  deliveries); case_01_native_rust PASSES.
- `cpp_c_param_live_read_e2e` (both arms) — component never publishes the
  baked param; `nros_cpp_get_param_integer` never reaches the callback.

**Others in the same confirmed set (not obviously the language class):**

- `entry_e2e` case_12_zephyr_rust_params — 60 s timeout, consistent.
- `realtime_tiers_e2e` case_08_nuttx_arm_cpp (`[TX] TIMEOUT: no
  completion`), case_09_nuttx_arm_c (timeout) — rust case_10 passes
  (flaky-pass under load).
- `emulator::test_qemu_rtic_action_e2e` — goal not accepted, 3/3; the rtic
  pubsub/service siblings pass (flaky under load only).

## Why this smells like ONE root (or few)

The C/C++ arms share the entry codegen (`emit_c.rs` / `emit_cpp.rs`), the
cpp-ffi layer, and the model→sched-context ingestion that Rust reaches via
a different path (`nros::main!`). Two recent events touched exactly that
surface:

1. `2ce930e39` (#356 Part2, 2026-07-30) — "author c/cpp/mixed embedded
   deploys + regenerate multi-board models".
2. The issue-0380 regenerations (models re-resolved; the deploy tables the
   C/C++ entry consumes changed shape) — the dims are restored, but the
   C-arm consumption may key on something the regeneration also changed.

Note this failure class sits exactly where RFC-0061 says tier 2 is blind:
platform×language pairing (1-wise sees each platform and each language,
never their product). This host's first pairwise-grade sweep is what
surfaced it.

## Triage guidance

Start with the ThreadX C pubsub cell (smallest: 0 messages, no QEMU for
threadx-linux, C entry TU inspectable in the fixture build dir): diff the
generated C entry TU + baked config against the Rust arm's, then check
whether the C entry ever opens its session (the issue-0380 threadx symptom
was "only the boot tier comes up" — if the C entry's tier table is empty,
the publisher task never spawns, which also explains action/service).
Verify against `git show 2ce930e39` for the deploy-table consumption
change, and against a pre-2ce930e39 fixture build if needed.

The param_live_read pair and the native realtime C/C++ arms likely fall
out of the same fix; re-run the confirmed set after.
