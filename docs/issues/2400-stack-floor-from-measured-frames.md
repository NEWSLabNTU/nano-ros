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

## Measured: a static floor alone cannot be both sound and tight (2026-11-10)

`scripts/nros-stack-floor.py` walks a thread role's deepest path from the
compiler's frames. The inputs are `.stack_sizes` (rustc `-Z emit-stack-sizes`),
GCC `.su` (Zephyr `CONFIG_STACK_USAGE`), call edges taken from relocations,
and indirect-call sites taken from the disassembly.

**It agrees with issue 2398 on direct calls.** On `nros-cpp` for thumbv7em at
O3, counting direct calls only: OPEN (`nros_cpp_init`) 15,728 B and SPIN
(`nros_cpp_spin_once`) 3,592 B. Issue 2398 measured 15,788 B and 3,624 B.

**It cannot follow indirect calls.** On the nucleo-f767zi Rust zenoh action
server, the walk runs from `rust_main` over all 25 archives plus 310 `.su`
files. Issue 2360's Renode twin measured that same code at 29,184 B:

| walk | OPEN | vs 29,184 B |
| --- | ---: | --- |
| direct calls only | 18,112 B | under |
| fold1, indirect targets scoped to zenoh/zpico | 26,232 B | under by 2,952 B |
| fold1, every address-taken target | 45,776 B | 57 % over |
| fold to a fixed point | does not converge | zenoh's callbacks reach their own dispatch |

Every indirect site is charged the same bytes: the deepest address-taken
target in scope. With no scope, that target is the graph query
`ZenohSession::names_and_types_filtered` (19,656 B frame), which this image
never calls. It is charged at the log sink's call site as well as at
session open. No static input binds a call site to the slot it loads. That
is issue 2402.

**Ruling.** A floor is `max(scoped static walk, measured twin peak)` plus a
stated margin. A role with no twin (PX4's executor-pass thread) gets the
static walk plus a stated per-backend indirect allowance, and that allowance
names the issue that would retire it (2402). The static half catches a frame
that grows on a direct path at build time. The twin covers the indirect
half.

**What real images needed** (all now in the walker):

- `-Zbuild-std`. The prebuilt rustup `core`/`alloc`/`compiler_builtins`
  carry no `.stack_sizes`, which left about 40 reached functions with no frame.
- Stated cuts for the panic path and for `core::fmt`, which really is
  recursive through `dyn Write`.
- GCC naming. A symbol `foo.isra.0` has its `.su` entry under `foo.isra`, and
  identical-code folding gives one body several names with a single `.su`
  line.
- `--frame` for the C that cargo's `cc` compiles under Zephyr. Zephyr's cmake
  sets `CFLAGS_<triple>`, so `-fstack-usage` cannot be added there.
- Fold now refuses a target whose own walk is refused. It used to skip that
  target, and that is how `zpico_open` dropped out silently.

The per-target table is `config/stack-floors.toml`. Its first entry is the
image above, OPEN: static 26,232 B, measured 29,184 B, margin 10 %, floor
32,104 B, which fits inside the board's 32 KiB. `--validate-table` checks it
offline, and `--table --entry` reproduces 26,232 B on the built image. How
the measure lane runs, and how the floor reaches the sizes header, is still
open.
