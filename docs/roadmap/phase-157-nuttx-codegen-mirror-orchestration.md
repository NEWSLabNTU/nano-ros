# Phase 157 — NuttX codegen mirror + Phase 155.F4 follow-ups

**Goal.** Unblock NuttX cmake C/C++ builds from fresh state + land
the two Phase 155.F4 follow-ups (action_core retry-on-empty-reply
and wait_for_action_server fast-path tighten) that were attempted
but rolled back because the underlying build orchestration broke.

**Status.** Open.

**Priority.** P1. NuttX C/C++ examples currently rely on
incremental-build leftovers to find `nros_config_generated.h`; a
fresh `rm -rf build` + `cmake -S . -B build && cmake --build` fails
on every example. Phase 155.F4 (`f6442f24`) only worked because the
test ran on a populated build tree.

**Depends on.** Phase 155 (rtos_e2e closed); Phase 156 (closed).

## Symptom

```
$ cd examples/qemu-arm-nuttx/c/zenoh/action-client
$ rm -rf build && cmake -S . -B build -DNUTTX_DIR=...
$ cmake --build build --parallel
…
/home/aeon/repos/nano-ros/packages/core/nros-c/include/nros/nros_config_generated.h:26:2:
  error: #error "nros_config_generated.h must be supplied per-build by the build system; see this stub for guidance."
/home/aeon/repos/nano-ros/packages/core/nros-c/include/nros/nros_generated.h:928:20:
  error: 'SESSION_OPAQUE_U64S' undeclared here (not in a function)
… (10 more undefined opaque-size identifiers)
gmake[1]: *** [CMakeFiles/example_interfaces__nano_ros_c.dir/all] Error 2
```

Affects every NuttX C/C++ example fresh build. Subsequent retries
fail identically — no recovery path.

## Root cause

Phase 155.B.4 commit `bfdd4c0a` ("fix: restore cross
build-test-fixtures path after Phase 140 install rip-off")
introduced:

```cmake
if(NOT NANO_ROS_PLATFORM MATCHES "^nuttx")
    add_subdirectory(packages/core/nros-rmw-cffi)
    add_subdirectory(packages/core/nros-c)
    add_subdirectory(packages/core/nros-cpp)
endif()
```

Skipping the corrosion-driven build of `nros-c` / `nros-cpp` on
NuttX (since the FFI crate's cargo handles them via path-deps with
build-std). But that also skipped:

- The `cargo-build_nros_c` POST_BUILD command that mirrors
  `${CMAKE_CURRENT_BINARY_DIR}/nros_config_generated.h` into
  `${BINARY_DIR}/include/nros/nros_config_generated.h`.
- The `cargo-build_nros_cpp` analogue for the C++ header.

The codegen pipeline at
`packages/codegen/.../NanoRosGenerateInterfaces.cmake:526` then
`add_library(${pkg}__nano_ros_c STATIC ${_generated_sources})` —
host gcc compiles the generated C sources, which `#include
<nros/nros_config_generated.h>`, hitting the source-tree `#error`
stub.

The host-built codegen `.a` is dead weight on NuttX: the real .a
that gets linked into the NuttX kernel ELF is produced by
`nros-nuttx-ffi`'s cargo build via `APP_EXTRA_SOURCES` (the
generated `.c` files are also passed directly to the user's
`add_executable` and ferried through `nros_board_link_app` →
`APP_EXTRA_SOURCES`).

## Two paths

### Path A — restore corrosion for nros-c/nros-cpp on NuttX

Pull `add_subdirectory(packages/core/{nros-c,nros-cpp})` back into
the NuttX path with a corrosion `.cargo/config.toml` carrying
`-Z build-std=core` + nightly pin. Same pattern as the codegen
FFI's per-package config generation at
`NanoRosGenerateInterfaces.cmake:466`.

**Pros:** restores the existing per-build header mirror flow;
keeps the cmake target `nros_c-static` available for the umbrella
`NanoRos::NanoRos`.

**Cons:** corrosion's cross-build of nros-c is slow + duplicates
work the FFI cargo already does. nros-rmw-zenoh-staticlib was
explicitly skipped for the same reason at
`CMakeLists.txt:170` ("staticlib add_subdirectory for NuttX
because it requires `-Z build-std` that the example's per-FFI
cargo build supplies"). Re-adding corrosion for nros-c puts us
back in a similar bind.

### Path B — INTERFACE-only codegen on NuttX (preferred)

Don't host-compile the codegen `.a` on NuttX. Make
`${pkg}__nano_ros_c` an INTERFACE library carrying include dirs
only. The `.c` sources still reach the final ELF via the user's
`add_executable` + `APP_EXTRA_SOURCES`.

**Files:**
- `packages/codegen/packages/nros-codegen-c/cmake/NanoRosGenerateInterfaces.cmake`
  — for the C-codegen branch (`add_library(${_lib_target} STATIC ${_generated_sources})`),
  emit INTERFACE on NuttX. Adjust `_link_type` to INTERFACE
  whenever `${_lib_target}` is an INTERFACE library.

**Verify:** the mirror header is no longer needed for the cmake
side. The `.c` sources still reach the cargo-driven cross-build
which has the right header path via `nros-nuttx-ffi/build.rs`'s
`CARGO_TARGET_DIR/nros-c-generated/` include (set when nros-c's
own build.rs runs as a path-dep).

But cargo's nros-c probe must successfully extract sizes from
the `nros` rlib for the per-build header to materialise. Need
to verify the cargo dep tree completes for nros-nuttx-ffi:

- `cargo build` from `nros-nuttx-ffi` crate dir builds the full
  closure: nros → nros-node → nros-c → … → nros-nuttx-ffi.
- nros-c's build.rs runs after `nros` rlib is produced; its
  probe finds the rlib at
  `CARGO_TARGET_DIR/armv7a-nuttx-eabihf/release/deps/libnros-*.rlib`.
- Probe extracts sizes → writes
  `CARGO_TARGET_DIR/nros-c-generated/nros/nros_config_generated.h`.
- nros-nuttx-ffi's build.rs adds
  `CARGO_TARGET_DIR/nros-c-generated/` to cc-rs's include path
  (already in place since Phase 156-F2).
- main.c compiles with real header.

**Risk:** Path B Attempt-#2 (this session, rolled back) showed
the cargo build halting before `nros` rlib appears — fs-watch
probe times out → 0-size sentinel → stub still wins, this time
inside the FFI cargo build. Need to bisect: is it a build.rs
ordering issue, a missing dep, or build-std missing some
dependency that prevents `nros` from compiling?

## Sub-bugs (open work)

### 157.1 — INTERFACE codegen lib + restore F4 follow-ups

Re-apply Attempt-#2 (Path B) cleanly. Steps:

1. `NanoRosGenerateInterfaces.cmake`: emit
   `add_library(${_lib_target} INTERFACE)` on NuttX with
   `target_include_directories(${_lib_target} INTERFACE …)`.
2. Adjust `_link_type` selection block (lines 543-595) to pick
   INTERFACE when `${_lib_target}` is INTERFACE.
3. Verify fresh-tree NuttX C build of `nuttx_c_action_client`
   succeeds in one `cmake --build` pass.
4. Add `send_goal_blocking` retry-on-empty-reply (Phase 155.F4
   follow-up #1) — 5 attempts × 1s budget. Removes the need for
   the example-level warm-up.
5. Remove warm-up spin from `examples/qemu-arm-nuttx/c/zenoh/
   action-client/src/main.c` (kept by `f6442f24`).
6. Verify rtos_e2e NuttX 9/9 still PASS.

### 157.2 — Tighten `wait_for_action_server` fast-path

Phase 155.F4 root-cause: `is_server_ready()` flips true based on
zenoh-pico's local matched-entity tracking. The local tracker
sees the server's SS liveliness token, but the matching
`queryable` declaration hasn't been routed to zenohd yet.

Options:
- (a) Add a settle-period spin (e.g. 500ms) inside
  `wait_for_action_server` after the FIRST observation flips
  `server_seen` true. Skip on the latched fast-path (long-running
  clients).
- (b) Add a real wire-level probe: send a `service_server_available`
  query (currently returns `Ok(self.server_seen)` — wire it
  to actually query zenohd).
- (c) Document the limitation, leave the warm-up in place; mark
  fast-path as "approximate readiness, settle in caller".

**Recommended:** (a) — minimal change, covers all callers
(Rust + C + C++), no extra round-trip.

### 157.3 — Bisect why `nros` rlib doesn't build in Path B attempt

**Root cause found 2026-05-18.** Cargo build ordering puts
`nros-nuttx-ffi`'s build.rs (the `cc-rs` compile of `main.c` →
`#include <nros/nros_config_generated.h>`) BEFORE
`nros-nuttx-ffi`'s runtime deps (which include `nros-c` whose
build.rs WOULD write the per-build header). The order is:

```
1. build-std: core / compiler_builtins / alloc / std for target
2. nros-nuttx-ffi's build-deps for HOST (cc-rs, etc.)
3. nros-nuttx-ffi's build.rs runs on HOST
   → compiles main.c with cc-rs (targets ARM)
   → main.c #include <nros/nros_config_generated.h> fails on stub
4. (never reached) nros-nuttx-ffi's runtime deps for TARGET
   → nros-c built here, nros-c's build.rs would write per-build header
5. (never reached) nros-nuttx-ffi crate itself
```

The earlier "NuttX C builds work" observations were incremental
re-builds: prior cmake-driven corrosion runs in OTHER example
builds had left the per-build header at
`build/cmake-*-zenoh/cargo/nano-ros_*/nros-c-generated/...` —
plus the cached `cargo-target/nros-c-generated/` from a prior
nros-nuttx-ffi build leftover when nros-c's build.rs DID
eventually run (in step 4 of a *previous* invocation that was
NOT racing main.c).

**The Phase 155.F4 `f6442f24` commit only worked because the
test was run with a populated build tree, not from `rm -rf`
clean state.**

**Fix options:**

(a) **Make nros-c a build-dep of nros-nuttx-ffi.** Forces
    nros-c's build.rs to run BEFORE nros-nuttx-ffi's build.rs.
    Caveat: build-deps run for HOST, so nros-c's probe finds
    the HOST nros rlib (not TARGET). Sizes are nominally
    host-arch but for `#[repr(C)]` structs the layout matches
    target close enough — alignment differences are sub-word.
    Verify with sample sizes from
    `build/cmake-threadx-riscv64-zenoh/cargo/.../nros_config_generated.h`.

(b) **Move the per-build header generation out of nros-c's
    build.rs into a top-level cmake helper.** On NuttX, cmake
    pre-generates a SAFE fallback header before any cargo
    invocation. Loses per-target precision but trades
    correctness for ordering simplicity.

(c) **Pre-commit a NuttX-flavoured `nros_config_generated.h`
    fallback** at `packages/core/nros-c/include/nros/` and have
    the source-tree stub `#include` it conditionally
    (`#ifdef NROS_PLATFORM_NUTTX`). Sizes hard-coded from a
    prior successful cross-build. Bumps only when a backwards-
    incompatible size change lands.

Recommend (a) — minimal invasion, cargo handles ordering.

## Acceptance

1. Fresh-tree `cmake --build` of any NuttX C / C++ example in
   `examples/qemu-arm-nuttx/{c,cpp}/zenoh/*/` succeeds in one
   pass.
2. `cargo nextest run -p nros-tests --test rtos_e2e
   'platform_2_Platform__Nuttx'` → 9/9 PASS, action C still
   green WITHOUT the example-level warm-up.
3. send_goal_blocking handles empty-reply via retry (verified
   by removing the warm-up + still passing).

## Notes

The Phase 155.F4 warm-up is a workaround that hides this build-
orchestration bug + the wait_for_action_server fast-path bug
behind a 5s sleep. Closing this phase removes the workaround
+ surfaces the real fixes.
