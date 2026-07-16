---
id: 217
title: "`just zephyr build-fvp-aemv8r` unbuildable: phase-221 dropped the west source arg and the original app (rust/dds/talker) is retired"
status: resolved
resolved_in: "2026-07-17 — maintainer decision: RETIRE. build-fvp-aemv8r + run-fvp-aemv8r recipes deleted; book page updated; the AArch64-rust-compiles purpose is covered by build-fvp-aemv8r-cyclonedds-rust (in build-fvp-all). A zenoh-on-FVP lane, if ever wanted, is new work (board conf lacks POSIX for zenoh-pico)."
type: bug
severity: low
area: zephyr
related: [issue-0216, phase-217, phase-221]
---

## Problem (found 2026-07-16, while wiring `build-fvp-all` for #216)

The base FVP smoke lane fails on any fresh run:

```
ERROR: source directory "." does not contain a CMakeLists.txt; is this
really what you want to build? (Use -s SOURCE_DIR to specify the
application source directory)
```

The original recipe (d310f192c, phase 117.13) built
`examples/zephyr/rust/dds/talker`. The phase-221 track-A refactor
(09dcd2620) dropped the source-dir argument from the `west build`
invocation, leaving it dependent on an already-configured
`build-fvp-aemv8r-talker/` cache — and the original app dir has since been
retired entirely. Any machine without the museum build dir cannot run the
lane; `run-fvp-aemv8r` (which consumes its ELF) is dead with it.

## Not a simple re-point

Tried: `examples/zephyr/rust/talker` (the modern zenoh rust talker) fails
on this board — zenoh-pico's zephyr platform header needs the POSIX API
(`pthread_t` etc.), which the FVP AEMv8-R SMP conf doesn't enable. Making
zenoh work on this board is a porting task, not a recipe fix.

## Direction

Decide the lane's identity first:
- If its purpose was "rust compiles for AArch64", it is REDUNDANT with
  `build-fvp-aemv8r-cyclonedds-rust` (#216, green again) — retire the
  recipe + `run-fvp-aemv8r` instead of fixing them.
- If a zenoh-on-FVP lane is wanted, port the board conf (POSIX API +
  zenoh-pico Kconfig set) and re-point the recipe at
  `examples/zephyr/rust/talker`.

`build-fvp-all` (added by #216) deliberately excludes this lane until the
decision lands.

## RESOLVED (2026-07-17) — retired

Maintainer chose retire (2026-07-17): the lane's only surviving purpose
("rust compiles for AArch64") is covered by the green
`build-fvp-aemv8r-cyclonedds-rust` lane, which is also the shape the ASI
reference consumer actually uses (cyclone RMW). Deleted `build-fvp-aemv8r`
and `run-fvp-aemv8r`, updated the `build-fvp-all` comment and the book's
ARM-FVP page. Phase-292 W1.a will add the FVP *workspace-Entry* lane —
the modern replacement for what this smoke once proved.
