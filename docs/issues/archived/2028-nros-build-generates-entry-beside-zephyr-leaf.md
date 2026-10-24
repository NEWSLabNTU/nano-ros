---
id: 2028
title: "`nros build` on a standalone Zephyr leaf generates a west application beside it, so C/C++ leaves fail \"No prj.conf\" and Rust leaves demand a selection facade"
status: resolved
type: bug
severity: medium
area: [cli, zephyr]
found: 2026-10-24
resolved: 2026-10-24
related: [1288, 1517, 0892]
---

## Measured

`nros sync && nros build zephyr_zenoh` in a standalone Zephyr leaf, on main
at a9f280e4fe:

- `examples/zephyr/cpp/listener`:

  ```
  nros build:   west application → …/listener/build/zephyr-zenoh/zephyr_zenoh_entry
  CMake Error at …/zephyr/cmake/modules/extensions.cmake:2818 (message):
    No prj.conf file(s) was found in the
    …/listener/build/zephyr-zenoh/zephyr_zenoh_entry
  ```

- `examples/zephyr/rust/listener`:

  ```
  Error: nros build: `zephyr_zenoh` needs a selection facade and there is none at
  …/listener/generated/nros-selection/zephyr_zenoh_entry.
  ```

A direct `west build` of the same leaf worked, so only the `nros build` road
was broken — for all 22 `system.toml` leaves whose `CMakeLists.txt` calls
`find_package(Zephyr)` (18 under `examples/zephyr/`, 4 under
`examples/templates/`).

## Cause

The west road's application choice (phase-470 W5.a, issue 1288) had two
arms: an explicit `[image.<id>] entry`, or GENERATE `build/<coord>/<id>_entry/`
unless a hand-written `src/<id>_entry` exists. A standalone leaf is neither: it
keeps `system.toml` beside its own west application, with `prj.conf` and
`boards/` next to it. So the generator wrote a second application with no
`prj.conf`, and west was pointed at that.

## Fix

A third arm, checked before generation: when the build root IS the bringup and
its `CMakeLists.txt` calls `find_package(Zephyr)`, that directory is the
application (`zephyr_leaf_application` in `cmd/build.rs`). A workspace bringup
never answers — it is not the root, and its `CMakeLists.txt` is an ament one.

Measured after: both leaves above build and link (`zephyr.exe` for
`native_sim/native/64`), merging the leaf's own `prj.conf`,
`boards/native_sim_native_64.conf`, `prj-zenoh.conf` and the image fragment.

A build dir configured by the broken road records the generated application,
and west then refuses the leaf ("Build directory … is for application …");
delete `build/zephyr-zenoh/` once. Only a dir that never configured can be in
that state.
