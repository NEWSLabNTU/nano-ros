---
id: 2236
title: "A NuttX make app as a nros project needs a project identity and a knob channel"
status: open
type: limitation
area: [nuttx, build, configure]
found: 2026-11-04
related: [phase-530, rfc-0120, issue-1883, issue-0488]
---

## Symptom

The NuttX `apps/external` make road (`integrations/nuttx/Makefile`,
`Make.defs`) cannot run `nros configure` for the application it builds, so
an image built there is sized by its Kconfig knobs, never by its census or
its declarations. RFC-0120 D5 lists NuttX make as a hooked host; phase-530
W5 found it has nothing to hook.

## Why (measured, phase-530 W5)

- The make road builds ONE thing: the runtime archive (`libnros_c.a` or
  `libnros_cpp.a`), from Kconfig alone -- `CONFIG_NROS_RMW_*`,
  `CONFIG_NROS_ROS_EDITION`, `CONFIG_NROS_C_API` / `CONFIG_NROS_CPP_API` --
  through `cmake/NanoRosImageCrate.cmake -P`.
- It carries no nros project: no `system.toml`, no image, and the user's
  application is not one of its inputs. The per-example staging that once
  made it one was retired (Phase 212.M-F.12; issue 1883 retired the
  per-bringup template).
- It has no channel for configure's outputs: no seed, no knob file and no
  `--config` reaches its cargo build.

So layer 0 has nothing to configure there. Ruling R36 (phase-530) records
the road as a GUEST road, sized by its Kconfig knobs.

## What making it a project needs

Two design choices, neither of which exists today:

1. **A project identity** -- where the application's `system.toml` is and
   which image it builds (for example a Kconfig string naming the app
   directory, plus an image id).
2. **A knob channel** -- how configure's answer reaches the archive's cargo
   build (for example the image's `nros-cargo.toml` passed with `--config`,
   as the cargo road does) and the app's own objects.

With both, `nros configure` becomes a prerequisite of the archive's
`context::` step, run once per build.

## Workaround

Build a NuttX image as a nros project on the CMake leaf road
(`nros build` of a leaf whose `[image.*] board` is a NuttX board), which is
hooked: phase-530 W5 measured `examples/qemu-armv7a-nuttx/c/talker` at one
configure, no re-run, census-tight knobs identical to `nros build`'s.

## Home

phase-530 (W5, Ruling R36) records the decision; the work belongs with the
hookless/IDE adapters RFC-0120 D8 leaves for a later phase.
