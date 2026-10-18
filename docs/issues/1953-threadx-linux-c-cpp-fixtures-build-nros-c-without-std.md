---
id: 1953
title: "Every threadx-linux C and C++ fixture fails to build nros-c with \"unwinding panics are not supported without std\" since the cmake road's image crate landed"
status: open
type: bug
area: [build, cmake, platform]
severity: high
found: 2026-10-18
related: [1297, 1299]
---

## Measured

- Run: nightly 39106651485 (schedule, 2026-10-18 07:10 UTC, head `48ea0a464`).
- Job: 117614285121, `threadx_linux`.
- Fixture build: `fixture-rows-keep-going: 12 fixture row(s) FAILED to build`.
  These are the C and C++ rows, for example
  `fixtures-build threadx-linux c zenoh: examples/threadx-linux/c/service-server rc=1`.
- Compile error, once per failing row:

```
   Compiling nros-c v0.6.0 (/__w/nano-ros/nano-ros/packages/api/nros-c)
error: unwinding panics are not supported without std
  = help: using nightly cargo, use -Zbuild-std with panic="abort" to avoid unwinding
error: could not compile `nros-c` (lib) due to 1 previous error
```

- Test result: `Real failures: 6 / 6`. Every C and C++ cell of
  `test_rtos_{pubsub,service,action}_e2e::platform_3_Platform__ThreadxLinux`
  failed within about 0.5 s, because the fixture was never built. All three
  Rust cells PASSED, so the Rust road is unaffected.

The same job on yesterday's nightly (39012587878, head `d06f951f3`) was
GREEN.

## Suspect

Between the two nightlies, two merges changed how a C or C++ image builds
its Rust runtime:

- `d0d295647e` (#1297, RFC-0071 D9). "The cmake road bundles the backend
  through a generated image crate" (`cmake/NanoRosImageCrate.cmake`). The API
  crate is now imported through a generated crate. That crate splits the
  features `nros_feature_set()` derived into API features and backend
  features, and maps platform features through the backend descriptor's
  `[rmw.provides.cargo.platform_features]`.
- `2ccf662c0d` (#1299, RFC-0071 D10). The header field rename. Its only
  missed emitter is already fixed in `48ea0a464`, which is the head of this
  run, and that one failed at compile time with a different message.

threadx-linux is the one ThreadX platform built for the HOST triple. Its
nros-c needs `std` (the host has no `panic_handler` of its own and builds
`panic=unwind`). The error means that nros-c, or a crate it pulls in, was
compiled `no_std` for an unwind target. A feature such as `std`, or the
board's std platform feature, is getting lost in the image crate's split or
platform mapping. This is read from the code, not measured.

## What it is NOT

- Not issue 1459 (`undefined reference to nros_board_network_wait`). That
  failure is at link time; this one is at nros-c compile time.
- Not issue 1353 (disk) or a runner death. The job reached a verdict.
- Not the `.zenoh` → `.transport` emitter (fixed in `48ea0a464`, the head here).

## What would close it

First, reproduce one row locally. Build
`examples/threadx-linux/c/service-server` with cmake and read the generated
image crate's `Cargo.toml` (under the build dir). Compare the features it
passes to nros-c and to the zenoh backend with the feature set the pre-#1297
road passed directly. The fix belongs where that feature is dropped, either
the split in `nros_image_backend` or the platform map. Acceptance: the
nightly `threadx_linux` job builds all 12 C/C++ rows, and its 9 e2e cells
pass.
