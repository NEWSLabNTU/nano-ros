---
id: 2354
title: "A generated Rust Zephyr image on Cyclone DDS probably needs CONFIG_CPP, which the generated prj.conf does not state (not measured)"
status: open
type: bug
severity: low
area: [cli, zephyr, rmw]
related: [phase-548]
found: 2026-11-07
---

**Home: [phase-548](../roadmap/phase-548-three-languages-every-board.md)**
(seen while doing W2; NOT MEASURED).

## What

Phase-548 W2 made the generated Zephyr entry `prj.conf`
(`west_app::render_prj_conf` in `packages/cli/nros-cli-core/src/builder/west_app.rs`)
state `CONFIG_CPP=y` and `CONFIG_STD_CPP17=y` for a C++ image only. A Rust
image gets no C++ line.

The hand-written Zephyr Rust leaves that use Cyclone DDS set `CONFIG_CPP` in
their `prj-cyclonedds.conf`, because the Cyclone RMW backend brings C++ into
the image. The generated Rust road states nothing equivalent, so a Rust
image scaffolded with `nros new --rmw cyclonedds` for a Zephyr board may fail
to build, or build with Zephyr's C++ defaults.

**Not measured.** No such image was built. This issue records the
observation so it is not lost; the first step is to build one and read its
`.config`.

## Done when

A Rust Zephyr image scaffolded with `--rmw cyclonedds` has been built: either
it builds and runs with the generated `prj.conf` (and this issue closes as
not a bug, with the evidence), or the generated `prj.conf` states what the
Cyclone backend needs, keyed on the RMW rather than the language.
