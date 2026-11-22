# Phase 587 — Several programs from one Zephyr package, one image each

**Status (2026-11-22). PROPOSED: not started, no owner.** Implements the part of
[RFC-0071 D20](../design/0071-rmw-backend-descriptor.md#amendment-2026-11-20-an-owned-entry-with-several-executables-d20)
that Zephyr does not have yet: an owned entry may build several programs, and
the run chooses one. phase-583 gave that to the native and RTOS carrier roads.
On Zephyr a second program is refused at configure (issue 2746), and this
phase lifts that refusal.

## Why

A west build is one Zephyr image. `find_package(Zephyr)` creates one `app`
target, linked whole-archive into one `zephyr.elf`, so it holds one `main`.
Every nano-ros program on this road is appended to `app`. The other RTOS roads
link each program as its own executable, which is why D20 gives them one
firmware per executable.

Zephyr builds several images in one build through **sysbuild**
(`ExternalZephyrProject_Add`, one child image per build dir). That is the
likely route: one child image per executable, each its own `app`, and the run
choosing which ELF to load.

## Work items

### W1 — Prototype: two programs, two sysbuild images

- **Status:** OPEN
- **What:** a two-program owned package on `native_sim`, built as one
  sysbuild child image per executable.
- **Acceptance:** both ELFs build and boot; measured, what sysbuild changes
  for the board, flash and `nros run` roads, which today take one west build
  dir.

### W2 — The roads take a program per image

- **Status:** OPEN
- **What:** `nros build`, `nros run --exec`, `flash` and `monitor` resolve a
  program to its child image's ELF. The configure-time refusal
  (`_nros_zephyr_claim_app_program`) is lifted for a sysbuild build.
- **Acceptance:** the two-program package runs either program with
  `--exec`; a single-program package keeps today's build dir byte for byte.

### W3 — Book

- **Status:** OPEN
- **What:** "Several programs in one package" drops its Zephyr limit.

## Not in this phase

- Several programs in ONE Zephyr image: one image has one `main`.
