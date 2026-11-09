---
id: 2365
title: "A Zephyr board's main-stack raise multiplied into every zenoh task stack, and the nucleo-f767zi C talker overflowed RAM, stopping tier 2"
status: open
type: bug
severity: high
area: [zephyr, ci, memory]
related: [2359, 2360, 2202, 2113, 2356, phase-548, phase-466]
found: 2026-11-09
---

**Home: [phase-548](../roadmap/phase-548-three-languages-every-board.md)**, where issue
2359's main-stack raise came from.

## Measured

Tier-2 runs 42393593998 and 42454057239 on newslab-118 stopped in the fixture
build: `build-nucleo-f767zi-c-talker-serial` failed to link with
``region `RAM' overflowed by 7872 bytes`` (384 KiB SRAM), so no tier-2 cell ran.

Local builds of that one row (`NROS_ZEPHYR_FIXTURE_FILTER=build-nucleo-f767zi-c-talker-serial
just zephyr build-fixtures`, Zephyr 3.7):

| commit | CONFIG_MAIN_STACK_SIZE | RAM used |
|---|---|---|
| `9d0bd000a8` | 16384 | 351,936 B |
| `15ae9b72b0` | 16384 | fits |
| `6e35469bde` (main) | 32768 | 401,088 B (7,872 over) |

The whole +49,152 B is two objects in the link maps (every other symbol has
the same size under a new hash):

- the kernel's main stack (`kernel/init.c` `.noinit`): +16,384 B;
- `nros_thread_stacks` (`zephyr/nros_platform_zephyr_shims.c`): +32,768 B.

## Why

`c271cff625` (issue 2359) raised `CONFIG_MAIN_STACK_SIZE` 16 -> 32 KiB in the
nucleo-f767zi, rpi-pico-w and fvp-aemv8r-smp board confs, because a Rust entry
peaks at 28,280 B on main and a board conf cannot set the value per image.

On a zenoh image every task nano-ros creates (`zpico_exec`, `zpico_flush`) is
sized `NROS_ZEPHYR_STACK_SIZE`, which defaults to `CONFIG_MAIN_STACK_SIZE`
when `CONFIG_NROS_ZEPHYR_TASK_STACK_SIZE` is 0, as it was on all three boards.
So the C image paid 16 KiB of main stack plus 2 x 16 KiB of zenoh stacks it
never needed (the measured `zpico_exec` peak is ~2 KiB). The Kconfig help for
that knob already describes this coupling (s32k344 overflowed 21,588 B the same
way). What was missing was anything making a board that raises main state its
task stack too.

This is the shape issue 2202 describes (an image's RAM decided by defaults
nobody stated); here the default is the task stack's.

## Fix, part 1 (#3330)

The three boards 2359 raised (nucleo-f767zi, rpi-pico-w, fvp-aemv8r-smp)
stated `CONFIG_NROS_ZEPHYR_TASK_STACK_SIZE=16384`, their size before the
raise, with a fast gate (`zephyr-task-stack-stated`) and an allowlist for
mps2-an385 and qemu-cortex-a53. That put the nucleo C talker at 368,320 B of
393,216: it fit, but 16,384 B above the last green.

## Fix, part 2: the class

Part 1 patched three boards. The class is that the task stack inherited the
main stack at all, and that the main stack was a board fact when it is an
image fact. Both are fixed in the module, and part 1's lines and gate are gone:

- `CONFIG_NROS_ZEPHYR_TASK_STACK_SIZE` is its own symbol, outside
  `if NROS_RMW_ZENOH`, `default 16384` (`range 1024 1048576`); the
  `0 = inherit main` value is retired, from `zephyr/CMakeLists.txt` and from the
  shim's fallback. Cyclone keeps its 512 KiB via a `configdefault` in its block.
- `CONFIG_MAIN_STACK_SIZE` is a `configdefault` in the module, per image:
  32768 for a Rust image off `ARCH_POSIX` (issue 2359's measured 28,280 B
  peak), 16384 otherwise, in both the zenoh and the XRCE blocks. A board conf
  that states a main stack still wins (mps2-an385 131072, qemu-cortex-a53
  32768), and so does a leaf.
- The 52 `CONFIG_MAIN_STACK_SIZE=16384` leaf lines and the two scaffolds that
  wrote them (`cargo-nano-ros` scaffold, `nros new`) are gone, and so are the
  three boards' 32768 lines.
- `just check zephyr-task-stack-decoupled` replaces the allowlist gate: it
  holds the shim, Kconfig and CMake to "no inherit-main path", with no
  allowlist. `zephyr-rust-main-stack` now reads the module's Rust default in
  both RMW blocks.

RAM, before -> after (Zephyr 3.7, same fixture rows):

| image | before | after |
|---|---|---|
| nucleo-f767zi C talker | 368,320 B | 351,936 B (the last green) |
| mps2-an385 C talker | 1,039,656 B | 810,280 B |
| mps2-an385 C++ talker | 1,038,632 B | 809,256 B |
| mps2-an385 Rust talker | 823,804 B | 594,428 B |
| qemu-cortex-a53 SMP C workspace | 1,732,612 B | 1,699,844 B |

## Is 16 KiB enough for the zenoh tasks? Measured

Zephyr's thread analyzer (`CONFIG_THREAD_ANALYZER`, `INIT_STACKS`), on the
part-2 images with a 16,384 B task stack, each run against the test's own
router:

| run | zpico_exec peak | zpico_flush peak | main peak |
|---|---|---|---|
| mps2-an385 C talker, `zephyr_cortex_m_talker_keeps_one_session_past_the_lease` (75 s, 144 publishes, 11 samples) | 1,304 B | 168 B | 13,472 / 131,072 B |
| qemu-cortex-a53 SMP C workspace, `sched_dims` CorePinPlacement cell (~30 s) | 2,416 B | not legible | 18,032 / 32,768 B |

The a53 console interleaves the two cores' output, so one `zpico_exec` line of
five dumps was legible; it is a high-water mark, read about 20 s in. Both
peaks sit under 15 % of the stack. mps2's main peak (13 KiB of 128 KiB) says
that board's 131072 is a separate question, not taken here.

## Still open

- Issue 2360 (Rust Zephyr registration frames against 32 KiB) is related and
  unowned; not taken here.
- The a53 SMP console interleave can split a test's marker line, so the
  CorePinPlacement cell failed one run in four locally. Not filed yet.

## What would close it

A tier-2 run on main that builds `build-nucleo-f767zi-c-talker-serial` and
reaches the cells.
