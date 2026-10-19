---
id: 1954
title: "FreeRTOS C++ fixtures read STALE against a SIBLING leaf's generated image crate source"
status: resolved
type: bug
area: [testing, build, cmake]
severity: high
found: 2026-10-18
related: [1297, 1953, 0445]
---

## Measured

- Run: nightly 39106651485 (schedule, 2026-10-18 07:10 UTC, head `48ea0a464`).
- Job: 117614285185, `freertos`, step `just freertos test`.
- Result: `Real failures: 3 / 3 total failures`. The three are
  `test_rtos_{pubsub,service,action}_e2e::platform_1_Platform__Freertos::lang_3_Lang__Cpp`.
  Each failed three tries in about 5 s. Rust and C on the same platform passed.
- The previous three `freertos` nightly jobs were green: 117338455622 (2026-10-17),
  116859246510 (2026-10-16) and 116298393089 (2026-10-15).
- Panic text, from the `nightly-freertos-junit` artifact. It is the same shape for all three tests:

```
failed to resolve the freertos cpp Pubsub first fixture: BuildFailed("Test fixture is STALE — a source is newer than the built binary:
  binary: /__w/nano-ros/nano-ros/examples/mps2-an385-freertos/cpp/talker/build-zenoh/cpp_talker
  newer:  /__w/nano-ros/nano-ros/examples/mps2-an385-freertos/cpp/service-client/build-zenoh/nano_ros/packages/api/nros-c/nros-c-image/src/lib.rs
  probe:  examined 10976 input(s); exempted 34 regenerated-in-place header + 165 cargo OUT_DIR product + 0 config-header stamp beside its header; 11370 cargo input(s) answered by a byte-identical relink ...
```

The service and action tests name `cpp/action-client/build-zenoh/.../nros-c-image/src/lib.rs`
as the newer source of `cpp_service_server` and `cpp_action_server`.

## Reading

The "newer" file belongs to a different leaf. It is the generated image crate root, written at
configure time by `cmake/NanoRosImageCrate.cmake` (RFC-0071 D9, #1297). It lives in a
sibling's build directory, not the tested binary's.

The sibling was configured after the talker was built, in the same fixture pass, so its
`src/lib.rs` is legitimately newer. The talker never compiles that file.

The cargo target dirs are NOT shared. The log shows each leaf with its own target dir
(`cpp/talker/build-zenoh/cargo/nros-c-image_3637a` and
`cpp/service-client/build-zenoh/cargo/nros-c-image_b74e9`). So this is not the 0616
shared-target-dir class. The probe's input set reaches into sibling leaves' build output,
and nothing exempts the new generated `nros-c-image/src/` tree there.

## What it is NOT

- Not a stale-from-rebase artifact: fixtures and tests ran in the same job, on one tree.
- Not issue 1953. That one is a compile failure on threadx-linux; here every FreeRTOS C++ row
  built, and only the freshness probe refuses.
- Not a runtime defect: no fixture launched. Per 0445, a STALE verdict is absorbing, so the
  cells' real runtime result is unknown.

## What would close it

Two fixes are defensible.

- **Probe side:** stop the FreeRTOS C++ resolver from counting another leaf's `build*/` tree as
  an input of this binary. Or derive its inputs from the binary's own ninja/dep-info closure.
- **Producer side:** add an exemption through the one spelling
  `nros_tests::fixtures::staleness::exempt_probe_input`.

Either way, check why the C rows on the same platform were not hit; their leaves also generate
`nros-c-image`. Acceptance: the `freertos` nightly job runs the three C++ cells to a runtime
verdict.

## Resolution

Fixed on the probe side. The issue's reading was right about the symptom and
wrong about one fact: the cargo target dirs ARE shared, one level up.

**Cause.** Every cross-compiled leaf's `build-<rmw>/cargo` is a symlink into the
phase-340 group dir `build/corrosion-cargo/<platform>/<hash>/`, shared by every
leaf with the same feature key. Corrosion's `--target-dir` is
`<build>/cargo/<ws>_<hash>`, hashed on the workspace manifest path. Before
RFC-0071 D9 that manifest was the repository root for every leaf, so the group
held one folder (`nano-ros_1147c`) and walking all of it was walking the
leaf's own closure. D9 gives each leaf its own generated image crate root, so
the group now holds one folder per leaf. Measured locally after
`just freertos build-fixture-extras`: 30 folders (15 `nros-c-image_*`, 15
`nros-cpp-image_*`) under one group, of which `cpp/talker`'s `build.ninja`
names two. `cargo_unit_dep_files` and `find_build_script_outputs` walked
`<build>/cargo` whole, so they read every sibling's `.d`, and each sibling's
names its own generated `src/lib.rs`.

**Why C was not hit in the nightly.** Not a different mechanism. With the fix
disabled and a C++ sibling's `lib.rs` touched, both FreeRTOS C cells fail with
the identical verdict (`newer:` the C++ sibling's file). The nightly's C rows
escaped by build order only.

**Fix.** `CargoScope` in `packages/testing/nros-tests/src/fixtures/binaries/mod.rs`:
the folders under `<build>/cargo` a build owns are the `--target-dir` values its
own `build.ninja` passes whose grandparent is that build dir. Both walks skip
any other folder there. When `build.ninja` names none (no manifest, or an
unrecognised layout), everything is admitted, so an unknown layout fails toward
STALE, never toward FRESH. The leaf's FFI glue crate's
`--target-dir <build>/<pkg>/target` is outside the group and is unaffected.
NuttX and ThreadX cmake fixtures go through the same two walks.

**Verification.**

- Reproduced locally: touching `cpp/service-client`'s generated `lib.rs` made
  `cpp_talker` STALE with the nightly's exact message (examined 11,051 inputs).
  With the fix the talker runs, and a sibling cell's probe reports 1,249
  inputs examined (`cpp_service_client`), its own closure only.
- All six `test_rtos_{pubsub,service,action}_e2e::platform_1_Platform__Freertos`
  C and C++ cells reach a runtime verdict and pass. `cpp_service_client`
  correctly stays STALE against its OWN touched `lib.rs` until rebuilt.
- Unit test `a_sibling_leafs_image_crate_is_not_a_staleness_input` covers the
  sibling folder, the leaf's own edit, the build-script record walk and the
  no-manifest fallback. Mutation-tested: making the scope admit everything
  turns it red.
- Not run: the `freertos` nightly job itself.
