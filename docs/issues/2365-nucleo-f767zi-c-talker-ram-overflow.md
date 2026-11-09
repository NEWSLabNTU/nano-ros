---
id: 2365
title: "A Zephyr board's main-stack raise multiplied into every zenoh task stack, and the nucleo-f767zi C talker overflowed RAM, stopping tier 2"
status: open
type: bug
severity: high
area: [zephyr, ci, memory]
related: [2359, 2360, 2202, 2113, 2356, phase-548]
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

## Fix

- The three boards 2359 raised (nucleo-f767zi, rpi-pico-w, fvp-aemv8r-smp)
  state `CONFIG_NROS_ZEPHYR_TASK_STACK_SIZE=16384`, their size before the raise.
  mps2-an385 (128 KiB main) and qemu-cortex-a53 (32 KiB) are left as they
  were, accepted by name in the gate: a board conf merges after a leaf's (last
  wins, issue 0876), so a board task-stack line would beat a leaf that states
  its own, and `sched-task-stacks-dtcm-zephyr-mps2` does (8,192 B;
  `check-kconfig-overridden-values` caught exactly that in the first draft).
- `just check zephyr-task-stack-stated` (fast, buildless): a tracked conf that
  sets `CONFIG_MAIN_STACK_SIZE` above 16384 also sets a nonzero task stack.
  Confs named for a non-zenoh RMW are exempt (the symbol is inside
  `if NROS_RMW_ZENOH`), and so are the two accepted boards, each with its
  reason. Run against the tree before the fix, it names the three boards.

Measured after the fix: the nucleo-f767zi C talker uses 368,320 B of 393,216
(main 32768, task stack 16384), exactly 16,384 B above the last green.

## Still open

- The C and C++ entries fit a 16 KiB main stack; only Rust needs 32 KiB. A
  per-image main stack (Rust 32 KiB, C/C++ 16 KiB), stated by the module
  instead of the board, would return the C image's other 16 KiB. That is a
  follow-up (it has to work on both Zephyr lines, 3.7 and 4.4).
- Issue 2360 (Rust Zephyr registration frames against 32 KiB) is related and
  unowned; not taken here.

## What would close it

A tier-2 run on main that builds `build-nucleo-f767zi-c-talker-serial` and
reaches the cells.
