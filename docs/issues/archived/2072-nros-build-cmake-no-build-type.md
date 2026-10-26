---
id: 2072
title: "nros build configured cmake board images with no CMAKE_BUILD_TYPE, so they compiled unoptimised; one ported call reserved 2.2 MB of stack"
status: resolved
resolved: 2026-10-26
resolved_in: "phase-518"
type: bug
area: [cli, build, cmake, freertos]
severity: high
found: 2026-10-26
related: [2070, phase-518, phase-336]
---

## What

`nros build` writes the cmake configure line for a board image on two roads:
the single-package leaf (`cmd/build.rs`, the `-DNROS_IMAGE` arm) and the
workspace road. Neither line passed `CMAKE_BUILD_TYPE`, and nothing below it
sets a default: not the toolchain file, not `nano_rosConfig.cmake`, not the
board module.

An empty build type compiles C and C++ with no optimisation. The fixture road
(`fixtures-build.sh`) always passes `-DCMAKE_BUILD_TYPE=Release`, so every test
ran a differently compiled program from the one `nros build` hands a user.

## How it showed

Phase 518 node 1 (`autoware_external_cmd_selector`) on mps2-an385 FreeRTOS
hard-faulted on its first timer tick (CFSR `INVSTATE`, stacked PC 0). Its
`diagnostic_updater::Updater::publish_now()` started with:

```
sub.w  sp, sp, #2211840
subw   sp, sp, #2372
```

That is a 2.2 MB frame on a 64 KiB task stack. The frame held a
`DiagnosticArray` at the default message capacities (64-element sequences,
256-byte strings), so `-O0` was not the only cause. Without optimisation,
though, every temporary of that size gets its own slot.

## Fix

`board_build_type_arg` in `packages/cli/nros-cli-core/src/cmd/build.rs` passes
`-DCMAKE_BUILD_TYPE=Release` on both board roads, for every platform except
`posix`, whose images are host processes. It goes before the caller's
`native_args`, so `nros build -- -DCMAKE_BUILD_TYPE=Debug` still wins.

The message's size is a separate fix, owned by the porter: an
`nros-codegen.toml` (RFC-0033) beside the workspace, for example
`[packages."diagnostic_msgs"] sequence = 4, string = 64`. With both changes the
node runs against ROS 2 Humble, and its boot report reads
`app task stack peak 13768 of 65536 bytes`.
