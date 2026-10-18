---
id: 1957
title: "s32ds builds its nano-ros staticlib through the generated image crate since phase-506 W4a, and nobody has built it in the S32 Design Studio since"
status: open
type: limitation
area: [build, integrations]
severity: low
found: 2026-10-18
related: [issue-1297, phase-506]
---

**Home: [phase-506](../roadmap/phase-506-api-crates-carry-only-the-api.md)** — its W4c records this.

## What

`integrations/s32ds/CMakeLists.txt` builds the nano-ros runtime with
`add_subdirectory(<repo>)` and links `nros_c-static` or `nros_cpp-static`
(`NROS_S32DS_API`). That is the cmake road, so phase-506 W4a (RFC-0071 D9)
moved it with every other cmake consumer: the target is now built from a
generated image crate (`cmake/NanoRosImageCrate.cmake`) that depends on the API
crate without a backend feature and on `nros-rmw-zenoh` directly, instead of
`nros-c` / `nros-cpp` with `rmw-zenoh` / `rmw-zenoh-cffi`.

The S32 Design Studio was not available to the session that made the change, so
the s32ds build itself was not run.

## Why it is expected to work

The maintainer ruled (2026-10-18) to treat s32ds as covered by the FreeRTOS
cmake-road verification rather than give it a special path: the image crate
sees only the API crate's features and the descriptor's platform table, and
s32ds's own inputs (its FreeRTOS and lwIP libraries) never reach it. W4a built
the FreeRTOS `mps2-an385` C and C++ workspace entries through the same
generator, with `platform-freertos` forwarded to the backend.

## Acceptance

`integrations/s32ds` configures and builds in S32DS (or with its CMake
toolchain file) for both `NROS_S32DS_API=C` and `CPP`, and the configure log
shows `nros-c image crate bundles nros-rmw-zenoh`.
