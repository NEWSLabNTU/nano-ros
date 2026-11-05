---
id: 2289
title: "The release ships no PX4 POSIX archives or placed headers, so a PX4 external module cannot build on the installed track"
status: open
type: enhancement
area: release, px4, cmake, book
purpose: [field, industrial]
related: [phase-537, phase-527]
found: 2026-11-05
---

**Home: none yet.** Phases 527, 528 and 531 do not cover it (CLI/build owner,
2026-11-05). The recommendation is a release-packaging work item: ship the
archives prebuilt, as `nros toolchain stage-packages` ships command packages.
Until a phase takes it, the book's PX4 page stays in User guide › Platforms,
waiting on this issue (maintainer ruling, 2026-11-05), and its `px4` probe
track carries an `awaits` naming it.

## Measured

A reader following the PX4 page (`book/src/guide/platforms/px4.md` after
phase-537 W1; `targets/px4.md` before) does, on the installed track:

1. clone PX4-Autopilot v1.17.0 and run its `Tools/setup/ubuntu.sh --no-nuttx --no-sim-tools`;
2. copy `integrations/px4/module-template` out of `$(nros locate nano-ros)`;
3. `make -C PX4-Autopilot px4_sitl_default EXTERNAL_MODULES_LOCATION=…`.

Step 3 stops at PX4's configure. `integrations/px4/NanoRosPx4Module.cmake`
resolves two prebuilt archives and the per-build headers, and the installed
SDK root has none of them:

- **`libnros_cpp.a`**, built for uORB (`--features alloc,env,log,panic-platform,rmw-cffi,platform-posix`);
- **`libnros_platform_posix.a`**;
- **the placed generated headers** (`nros_config_generated.h` and the C++ config
  header) under `_NROS_PX4_HEADER_DIR`. They must come from the same cargo
  invocation as the archive, because they carry storage sizes (the issue-0268
  class otherwise).

`_nros_px4_resolve_archive` fails with `FATAL_ERROR` "required nano-ros archive
not found", and its remedy is `_NROS_PX4_UORB_ARCHIVE_CMD`: a
`cargo build -p nros-cpp …` wrapped in
`scripts/build/cargo-out-dir-headers.py`, run against `NROS_REPO_DIR`. That is
a contributor step. The release's SDK root is a `git archive` with no build
outputs (`scripts/stage-sdk-root.sh`), so there is nothing to point
`-DNROS_CPP_ARCHIVE` at, and the book cannot tell a reader to run cargo inside
the SDK root (RFC-0117, RFC-0124 Principle 5).

A second, smaller gap is on the same path: a module that names a `BRINGUP`
calls `nros dev image model-path` and `nros dev codegen entry` from CMake. These
are dev-tier verbs, so a user build reaches the developer tier through PX4's
configure. The module template names no `BRINGUP`, so the book's path doesn't
hit this today.

## Why it matters

- The PX4 page cannot be replayed, so under RFC-0115 D4 and RFC-0124 Principle 3
  it is in the user book by ruling, not by evidence. The registry row
  `px4-sitl@Px4` stays "Boots, manual".
- Field users (drones are the field purpose's lead integration) meet a configure
  error whose fix is a cargo command against a checkout they don't have.

## Options

1. **Ship the archives prebuilt (recommended by the CLI/build owner).** The
   release builds `libnros_cpp.a` (uORB features), `libnros_platform_posix.a` and
   their paired headers for the PX4 POSIX host target, and stages them where
   `NanoRosPx4Module.cmake`'s defaults look. This is one host target, like the
   host tools. The paired-header check (`NanoRosArchivePairing.cmake`) keeps
   them honest.
2. **A user verb that builds them** (for example `nros px4 deps`). This makes
   every PX4 user build nano-ros's C++ runtime, and needs a Rust toolchain on a
   firmware host. That is heavier for the field purpose.

Prebuilt NuttX archives for Pixhawk targets (`px4_fmu-*`) are a separate,
later question: they are per-board cross builds.

## Done when

- The `px4` probe track in `packages/boards/board-support.toml` drops its
  `awaits` and replays `guide/platforms/px4.md` green on the installed track:
  the SITL `make`, the headless boot, and `px4-nano_ros_app start`.
- The PX4 page states no cargo step.
