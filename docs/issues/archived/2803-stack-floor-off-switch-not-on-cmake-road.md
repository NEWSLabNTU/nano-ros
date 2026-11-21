---
id: 2803
title: "`NROS_STACK_FLOOR=OFF nros build` still walks a cmake- or west-road image: the knob is an env var on the cargo road (ON/OFF) and a CMake cache variable on cmake (CHECK/OFF)"
status: resolved
type: bug
severity: low
area: [build, stack-floor, cmake]
related: [issue-2414, issue-0460, phase-557]
found: 2026-11-21
---

**Home: [phase-557](../../roadmap/phase-557-stack-floor-measured-frames.md).**

## What

The stack-floor check has one off switch, spelled two ways on two roads:

| road | carrier | values |
| --- | --- | --- |
| cargo (`nros build` of a cargo image) | the environment, `packages/cli/nros-cli-core/src/cmd/stack_floor.rs` | `ON` / `OFF` (and `0`, `false`, `no`) |
| cmake and Zephyr west | the CMake CACHE variable `NROS_STACK_FLOOR`, `cmake/NanoRosStackFloor.cmake` | `CHECK` / `OFF` |

So `NROS_STACK_FLOOR=OFF nros build` of a Zephyr image still walks it.
Measured on a `nros new` Rust talker for `nucleo-f767zi-zephyr`: the build
printed the walk and its verdict anyway. Its configure line tells the user
the opposite:

```text
-- nano-ros: stack floor of ... checked at build (...; NROS_STACK_FLOOR=OFF skips)
```

A value that is right on one road is an error on the other. With the
environment variable reaching cmake, `ON` fails the configure
(`NROS_STACK_FLOOR='ON' -- expected CHECK or OFF`).

This is issue 0460's class: a knob that reaches one lane and not the other,
because each lane has its own carrier.

## Fix

One vocabulary and one carrier on both roads:

- The cmake road reads `$ENV{NROS_STACK_FLOOR}` when the cache variable is
  not set on the command line. An explicit `-DNROS_STACK_FLOOR=` still wins.
- One value set everywhere: `ON` and `CHECK` are synonyms, and the off
  spellings are the cargo road's.
- A test on each road holds the other's spellings.
- The configure line names the spelling that works.

## Resolution (2026-11-21)

Fixed as proposed:

- **One carrier.** `cmake/NanoRosStackFloor.cmake` reads
  `$ENV{NROS_STACK_FLOOR}` unless a `-D` set the value. It remembers the
  value the environment set last, so a reconfigure sees the variable change
  or go away, and a `-D` value is never overwritten.
- **One vocabulary, case-insensitive:** on `ON`, `CHECK`, `1`, `TRUE`,
  `YES`; off `OFF`, `0`, `FALSE`, `NO`. The CLI holds it as `SWITCH_ON` /
  `SWITCH_OFF`. cmake refuses any other value; the CLI walks on one (the
  safe direction) and says so.
- **Tests.** The CLI holds the cmake module's two matchers to its lists. A
  real configure of a NONE-language project covers the default, env `OFF`,
  env `ON` on reconfigure, env unset again, and `-D` beating the
  environment. Mutation: with the module's environment branch removed, that
  test fails.
