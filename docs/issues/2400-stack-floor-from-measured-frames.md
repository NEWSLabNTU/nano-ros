---
id: 2400
title: "Three stack floors are each stated a different way: one is computed from a value size, one is a hand-set 32 KiB, one is a measured FreeRTOS peak. None is derived from the frames the thread actually runs"
status: open
type: tech-debt
severity: medium
area: [memory, zephyr, px4, nuttx, freertos, build]
found: 2026-11-10
related: [issue-2360, issue-2365, issue-2398, issue-2359, issue-0961, issue-1146, phase-548]
---

**Home: [phase-548](../roadmap/phase-548-three-languages-every-board.md)**, beside
issue 2360, whose measurements are this issue's main input.

## What

A thread that runs nano-ros registration or an executor pass needs a minimum
stack. Three places state that minimum today, and each states it differently:

| floor | where | how it is stated | value |
| --- | --- | --- | --- |
| `NROS_EXECUTOR_MAIN_STACK_MIN` | `nros-build-helpers/src/cpp.rs` and the `nros-c` sizes-header templates (issue 0961) | **computed**: 2 x `NROS_EXECUTOR_VALUE_SIZE`. That covers two copies of one value on the stack and nothing else | 3,808 B on fmu-v6x (`thumbv7em-nuttx-eabihf`) |
| `NROS_RUST_MAIN_STACK_FLOOR` | `zephyr/CMakeLists.txt` (issue 2359), checked by `check-zephyr-rust-main-stack` | **hand-set** from one Renode peak (a talker, 28,280 B) | 32,768 B |
| `app_stack_bytes` | `nros-board-common` `freertos_config` (issue 1146) | **measured** high-water over 8 FreeRTOS images, rounded up | 131,072 B |

The PX4 refusal in #3414 (issue 2398) reads the first floor. Issue 2398
measured that floor to be about a quarter of what the open path needs on that
target: the open path alone is ~15.3 KB before any backend frame.

So a thread a gate accepts can still overflow. The number a gate checks says
nothing about the frames that thread runs.

## Measured inputs that exist

- **Runtime peaks** (issue 2360, nucleo-f767zi Renode twin, `CONFIG_INIT_STACKS`
  plus the thread analyzer, Rust + zenoh, main thread, after #3381): talker
  27,248 B, action server 29,184 B, action client 30,600 B. These are 1.4 to
  4.7 KiB under the 32 KiB floor. A graph-querying image
  (`ZenohSession::names_and_types_filtered`, a 19,620 B frame) is not measured.
- **Static frames** (issue 2398, `-Z emit-stack-sizes` +
  `llvm-readobj --stack-sizes`, thumbv7em, O3/Oz): `Executor::open_in`
  2,840/2,872 B, `init_rmw_over` 2,128 B, `CffiRmw::open_with_rmw` 648/600 B,
  `open_marshalling_properties` 8,472/8,464 B, `nros_cpp_init` 56/24 B. One
  spin pass is 2.8 to 4.2 KB.
- **FreeRTOS** (issue 1146): `uxTaskGetStackHighWaterMark` at the end of the
  register pass, worst of 8 images 36,152 B. Every image prints its own peak at
  boot.

## Proposal

Keep **one producer** per target. Every gate (the Zephyr Rust main-stack check,
the PX4 thread-stack refusal, the C/C++ `#error` on `CONFIG_MAIN_STACK_SIZE`)
reads that producer's output and does not hold its own number.

1. **Static bound.** At build time, take the archive's `.stack_sizes` (already
   emitted for issue 2398's measurement) and sum the worst path over the
   registration and open call chains. That gives a per-target, per-feature
   value that moves when a frame moves. Recursion and indirect calls through
   the RMW vtable stop the sum. Each such call is stated as a named allowance
   with the measurement it came from, never left out silently.
2. **Runtime check.** The Renode nucleo twin (and the FreeRTOS boot print)
   measures a peak for each covered image. A lane asserts `static bound >=
   measured peak`. If the bound is under the peak, the static walk missed an
   edge. That is a defect in the walk, not a reason to raise a constant.
3. **Delivery.** The value goes in the per-build sizes header beside
   `NROS_EXECUTOR_VALUE_SIZE`, the same header #3414's
   `nros_px4_read_stack_floor` already reads (issue 1824). PX4 then needs only
   its producer changed, not its reader. Zephyr reads the same header instead
   of `set(NROS_RUST_MAIN_STACK_FLOOR 32768)`. `2 x NROS_EXECUTOR_VALUE_SIZE`
   stays as a necessary-condition `#error`. It stops being called the floor.

Open choices, left to whoever takes it:

- Whether the static walk runs in `nros-build-helpers` (it has the archive) or
  as a `nros` CLI step after link (it has the final image, so inlining is
  settled).
- Whether the floor is one per image, or one per thread role (registration /
  executor pass / work queue). PX4 has two roles on two threads (issue 2398);
  Zephyr Rust has one.

## Acceptance

- One producer of the floor value. `check-zephyr-rust-main-stack` and
  `check-px4-thread-stacks` read it, and neither holds a literal.
- On the three nucleo Rust images in the table above, the derived bound is at
  or above the measured peak, and within a stated margin of it.
- A frame grown on purpose (a test mutation) moves the derived floor, and a
  board conf below it is refused.
