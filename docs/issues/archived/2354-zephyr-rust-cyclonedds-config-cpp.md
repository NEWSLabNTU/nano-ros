---
id: 2354
title: "A generated Rust Zephyr image on Cyclone DDS probably needs CONFIG_CPP, which the generated prj.conf does not state (not measured)"
status: resolved
type: bug
severity: low
area: [cli, zephyr, rmw]
related: [phase-548]
found: 2026-11-07
resolved: 2026-11-09
---

**Home: [phase-548](../../roadmap/phase-548-three-languages-every-board.md)**
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

## Resolution (2026-11-09)

Measured, and worse than "may fail to build": `nros new zc --board zephyr
--lang rust --rmw cyclonedds` was REFUSED at configure. Two defects stacked.

1. **Kconfig could not take the choice.** The image's fragment
   (`cmd::leaf_kconfig`) stated `CONFIG_NROS_RMW_CYCLONEDDS=y`, but
   `NROS_RMW_CYCLONEDDS` `depends on NET_SOCKETS && POSIX_API && CPP`, and
   nothing in a Rust (or C) image states `CPP`. Kconfig kept
   `NROS_RMW_ZENOH` with a warning ("was selected (set =y), but
   NROS_RMW_ZENOH … ended up as the choice selection"), and
   `nros_check_image_agreement` then refused the configure: "image … declares:
   cyclonedds, Kconfig selected: zenoh". The fragment now states `POSIX_API`
   and `CPP` for a Cyclone image, keyed on the RMW and not the language, the
   same way it already states `POSIX_API` for zenoh and `RUST` for a Rust
   entry (issue 2189). Of the three RMW choices only Cyclone had an unstated
   dependency (`NET_SOCKETS`, XRCE's, is the board's networking).
2. **No backend was registered** (issue 2313's item 2). With the Kconfig
   fixed the image built, then stopped at `Executor::open` with "cannot select
   an RMW backend". `nros::main!`'s Zephyr arm called only the selection
   facade's gated Rust `register()`, which Cyclone, a C++ library the module
   compiles, does not have. The arm now also calls the module's C hook
   `nros_app_register_backends()`, as `nros::zephyr_component_main!` already
   did. That hook was a WEAK reference for every RMW, and a weak undefined
   reference never extracts an archive member: `libnros.a` defined
   `nros_rmw_cyclonedds_register` and `zephyr.exe` did not. For Cyclone,
   whose definition is in the module's own library, the hook is now strong.
   Zenoh and XRCE keep the weak guard issue 0163 needs.

Measured on native_sim/native/64 after both:

| image | result |
| --- | --- |
| `--lang rust --rmw cyclonedds` | `Publishing: 'Hello World: 1'` after 3.0 s |
| `--lang c --rmw cyclonedds` | `Publishing: 'Hello World: 1'` after 1.0 s |
| `--lang rust --rmw zenoh` (regression check) | `Publishing: 'Hello World: 1'` after 3.4 s |

The C image was not built before the fix, so whether it was refused too is
inferred from item 1, not measured.

Not taken: depending on `nros-rmw-cyclonedds-sys` from the generated entry.
Tried, and the link fails on `nros_cyclonedds_build_descriptor_from_schema`:
its `nros-rmw-cyclonedds` dependency references `bridge/dynamic_type_builder.cpp`,
which the Zephyr module does not compile (phase-531 W6 measured the same).

Seen on the way, not fixed here: the "cannot select an RMW backend — {why}"
line reached the console cut off at its em dash (a replacement character,
then the next record), so the reason never printed.
