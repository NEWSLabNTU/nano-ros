---
id: 1953
title: "Every threadx-linux C and C++ fixture fails to build nros-c with \"unwinding panics are not supported without std\" since the cmake road's image crate landed"
status: resolved
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

## Resolution

The suspect was the wrong half. Nothing dropped a feature. The image crate
dropped a PROFILE.

- **What changed in #1297.** Before RFC-0071 D9, the API crate was built as a
  member of the repository's root workspace, and the root `Cargo.toml`
  defines every profile it is built with. `cmake/NanoRosImageCrate.cmake`
  moved it into a GENERATED crate that is its own `[workspace]`, and that
  manifest stated no profile at all.
- **Why only `release` broke.** The `nros-*` presets survived, because
  `nros_cargo_profile_env()` injects them as `CARGO_PROFILE_*` variables.
  `release` and `dev` are never injected, by nros-cargo-profile's ownership
  rule. So `--profile release` got cargo's default release profile:
  `panic = "unwind"`, with no fat LTO.
- **Why threadx-linux.** `nros-c` declares `crate-type = ["staticlib",
  "cdylib", "lib"]`, so it builds those artifacts even as a dependency. A
  `no_std` staticlib on an UNWIND target is what rustc refuses: "unwinding
  panics are not supported without std". threadx-linux is the one `no_std`
  C/C++ tier on the host triple. Cross targets abort by target spec, so they
  compiled, and silently lost the root's `lto = "fat"` / `codegen-units = 1`.
- **Why the nightly and nothing else.** Only `threadx_linux
  build-fixture-extras` passes `-DCMAKE_BUILD_TYPE=Release`. Every other lane
  builds `nros-relwithdebinfo`, which is injected, so they stayed green.

Measured, on the generated `nros-c-image` of `examples/threadx-linux/c/talker`:

- the CI cargo command with `--profile=release` reproduces the error exactly;
- the same command with `--profile=nros-relwithdebinfo` builds.

**Fix.** `cmake/NanoRosRootProfiles.cmake` adds `nros_root_cargo_profiles()`.
It copies the root `Cargo.toml`'s `[profile.*]` tables into a generated root
at configure time, so the profiles are copied, not restated, and there is no
second table to drift.

- **Who calls it.** The image crate (standalone, and the shared root in
  `WORKSPACE_DIR` mode) and the workspace runtime umbrella
  (`NanoRosRuntimeCrate.cmake`, which had the same gap).
- **Exempt.** The C++ FFI glue template keeps its own block, with the reason
  recorded in the gate. It builds only at the `cpp-ffi-glue` carve-out, which
  is injected, and it also generates from an installed prefix that has no
  root manifest.
- **The parse.** It walks the manifest's lines by hand. A `file(STRINGS)`
  list split comment fragments that hold `;` or an unbalanced `[` into TOML
  keys. Measured: the first version failed `cargo metadata` with "key with no
  value".

**Gate.** `just check generated-root-profiles` (fast line) checks three
things:

- every cmake emitter of a `[workspace]` table calls the helper, or is exempt
  with a reason;
- the helper reproduces the root's profiles exactly;
- the image crate generator, run in script mode, writes a `release` and a
  `dev` that abort.

It was mutation-checked: dropping the image crate's call turns all three red.

**Acceptance, measured locally.**

- **Build.** `just threadx_linux build-fixture-extras` is the nightly's
  Release configure. Every one of the 12 zenoh C and C++ rows, and the
  cyclonedds rows, builds at `--profile=release` with no failure. The build
  dirs' `build.ninja` were checked for the profile.
- **Tests.** All 9 `test_rtos_{pubsub,service,action}_e2e` ThreadxLinux cells
  pass (C, C++ and Rust), on those release-built fixtures.
- **Not run.** The nightly itself. Its next `threadx_linux` job is the
  remote confirmation.

A side finding, not fixed here. `NROS_CARGO_PROFILE` is a cached FORCE
variable, so a build dir first configured without `CMAKE_BUILD_TYPE` keeps
its derived profile when a later configure passes `-DCMAKE_BUILD_TYPE=Release`.
That is why a local re-run of the nightly recipe over existing dirs did not
reproduce this. `cmake <dir> -DNROS_CARGO_PROFILE=` re-derives it.
