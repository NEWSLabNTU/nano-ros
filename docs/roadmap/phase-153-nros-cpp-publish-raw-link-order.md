# Phase 153 — `nros_cpp_publish_raw` Link-Order / Codegen-FFI Archive Gap

**Goal.** Resolve undefined `nros_cpp_publish_raw` references from
codegen-generated `nano_ros_cpp_ffi_<pkg>.a` archives at native
C++ example link time. Today every C++ example pulling a codegen
package (`std_msgs`, `example_interfaces`, `action_msgs`, etc.)
fails with ~30 `undefined reference to nros_cpp_publish_raw` calls
during the final ld step. Dominates Phase 150's post-140 failure
inventory: 42 native_api + 8 dds_api (50 of 136 total).

**Status.** Superseded — Phase 150.B closed this 2026-05-18 via
`5d00c930 phase-150.B: fix dds_api C++ builds — ffi_lib →
NanoRosCpp link order`. The fix added `NanoRos::NanoRosCpp` to the
per-package `${_lib_target}_ffi_lib` STATIC IMPORTED target's
`INTERFACE_LINK_LIBRARIES` in `cmake/NanoRosGenerateInterfaces.cmake`,
which records the ffi→cpp dep so CMake's topological sort places
`libnros_cpp.a` AFTER the ffi staticlib in the final link line.
Symbol resolves on the second pass.

Dds_api class (Phase 150.B, 6 tests) verified closed by the
user's commit. Native_api class (Phase 150.A, 42 tests) likely
also closed by the same fix since they share the codegen-FFI root
cause — pending CI verification on next run.

This doc preserved for the analysis it captures (option A vs B vs
C vs D); the actual fix landed as option A (NanoRosCpp on
INTERFACE_LINK_LIBRARIES) rather than option D (codegen template
dep) which this doc recommended. Option A is more surgical —
single cmake file, no codegen template churn.

**Priority.** P1 — 50 of 136 test failures from one root cause.
Highest ROI of any open phase right now.

**Depends on.** None blocking. Phase 144 (add_subdirectory
migration) introduced the surface; Phase 137/138 platform module
composition is downstream.

**Related.** Phase 134, Phase 144, Phase 149, Phase 150, Phase 151.

---

## Symptom

```
/usr/bin/ld: nano_ros_cpp_ffi_example_interfaces/target/release/
  libnano_ros_cpp_ffi_example_interfaces.a(...):
  undefined reference to `nros_cpp_publish_raw'
... (~30 such refs per example)
collect2: error: ld returned 1 exit status
```

Surfaces on every C++ example that calls `nano_ros_generate_interfaces(... LANGUAGE CPP)`:
- `examples/native/cpp/zenoh/{talker,listener,service-*,action-*}`
- `examples/native/cpp/dds/{talker,listener,service-*,action-*}`
- The codegen-generated `nano_ros_cpp_ffi_<pkg>` cargo crate compiles fine; the final `cmake --build` link step fails.

## Root cause hypothesis

`nano_ros_generate_interfaces` produces a Rust crate at
`<example>/generated/nano_ros_cpp_ffi_<pkg>/` that gets built via
corrosion into `libnano_ros_cpp_ffi_<pkg>.a`. That archive calls
`nros_cpp_publish_raw` (defined in `packages/core/nros-cpp/src/lib.rs`
exposed via the `nros-cpp` staticlib).

Pre-Phase-140 install path: `libnros_cpp.a` was install-staged at
`build/install/lib/`, and the example's `target_link_libraries`
pulled `NanoRos::NanoRosCpp` (IMPORTED target pointing at the
installed `libnros_cpp.a`). Link order in CMake's generated link
line put nros-cpp AFTER the codegen FFI archive — ld resolved the
back-reference.

Post-Phase-140 `add_subdirectory` path: nros-cpp's static archive is
built via corrosion from the root CMake. The codegen-FFI archive
gets built by `nano_ros_generate_interfaces` and linked into the
example. But the linker order now puts the codegen FFI BEFORE
nros-cpp's symbols → ld single-pass left-to-right doesn't resolve
the back-reference.

OR: the example's `target_link_libraries(<app> PRIVATE
<pkg>__nano_ros_cpp NanoRos::NanoRosCpp)` puts `<pkg>__nano_ros_cpp`
first; ld sees its `nros_cpp_publish_raw` reference but no later
archive in the link line provides the impl, because
`NanoRos::NanoRosCpp` is INTERFACE → CMake may not generate
explicit `-lnros_cpp` at the right position.

## Fix options

### A. Force NanoRos::NanoRosCpp into whole-archive link

Same pattern as Phase 134/144's zenoh/dds RMW staticlib wrap:

```cmake
target_link_libraries(NanoRosCpp INTERFACE
    "-Wl,--whole-archive" nros_cpp-static "-Wl,--no-whole-archive"
    "-Wl,--allow-multiple-definition")
```

Pro: same shape as RMW staticlib treatment. Catches every
codegen-FFI back-reference.
Con: blows up final binary size (~MB of unused symbols pulled in).

### B. Reorder via `target_link_libraries` PRIVATE → after FFI

In each per-example CMakeLists:

```cmake
add_executable(my_app src/main.cpp)
target_link_libraries(my_app PRIVATE
    <pkg>__nano_ros_cpp
    NanoRos::NanoRosCpp
    <pkg>__nano_ros_cpp)   # appear AFTER NanoRos to satisfy late refs
```

Or `--start-group / --end-group` around the pair.

Pro: targeted, no whole-archive overhead.
Con: per-example wiring; verbose.

### C. Make NanoRos::NanoRosCpp interface declare `nros_cpp_publish_raw` symbol-listing

If the issue is that ld doesn't pull `libnros_cpp.a` into the link
at all (no upfront reference), add a dummy `extern` symbol pull-in
from `NanoRos::NanoRosCpp`'s INTERFACE_LINK_OPTIONS.

Pro: invisible to consumers.
Con: hack; symbol-pin maintenance.

### D. Codegen-FFI archive declares dep on nros-cpp

The codegen-generated `nano_ros_cpp_ffi_<pkg>` cargo crate has a
`[dependencies] nros-cpp = { path = "..." }` entry; cargo
threads the link of `libnros_cpp.a` through naturally. Verify
this is the case + fix if missing.

Pro: cargo handles link order via its own dep tree.
Con: changes codegen output shape; needs `cargo nano-ros generate-cpp`
update.

**Recommend D first** — investigate whether codegen-FFI already
declares nros-cpp as a dep. If yes, fix elsewhere. If no, add it
to the codegen template. Falls back to A if D doesn't pan out.

---

## Work Items

- [ ] **152.1 — Reproduce + diagnose.**
      Run `cmake --build examples/native/cpp/zenoh/talker/build`
      verbose; capture the exact `cc` link command + the symbol
      table of the codegen-FFI archive (`nm libnano_ros_cpp_ffi_std_msgs.a |
      grep publish_raw`). Confirm whether nros-cpp is in the link
      line OR not.
      **Files.** none (diagnosis).

- [ ] **152.2 — Codegen template inspection.**
      Look at `cargo-nano-ros generate-cpp` template:
      `packages/codegen/packages/nros-codegen-cpp/templates/` (or
      equivalent). Check if generated `nano_ros_cpp_ffi_<pkg>/Cargo.toml`
      declares `nros-cpp` as a dep.
      **Files.** `packages/codegen/packages/nros-codegen-cpp/`.

- [ ] **152.3 — Implement Option D (preferred) OR Option A.**
      Per 152.1/152.2 findings.
      **Files.** TBD per option.

- [ ] **152.4 — Smoke verify on all 50 failing tests.**
      ```
      cd examples/native/cpp/zenoh/talker && rm -rf build && cmake -S . -B build && cmake --build build
      ```
      Repeat for action-server, action-client, service-server,
      service-client, listener. Plus DDS variants. All 6+12 must
      link clean.
      **Files.** none (verification).

- [ ] **152.5 — Re-run `just ci` to confirm drop.**
      Expected: failure count drops from 136 to ~86 (50 fixed).
      Remaining classes (qemu_patched_binary, cmake_platform_matrix,
      env-gated integrations) need separate work.
      **Files.** none (verification).

---

## Acceptance

- [ ] Every `examples/native/cpp/{zenoh,dds}/*` example builds
      end-to-end via `add_subdirectory` path; final link has zero
      `nros_cpp_publish_raw` undefined references.
- [ ] `just ci` `test-all` failure count drops by ~50.
- [ ] No regression in C examples (Phase 144.1/.2 work).
- [ ] No regression in already-passing cpp examples (parameters,
      custom-msg).

---

## Notes

- **Why missed by Phase 144.4.** The agent's spot-build verified
  C++ talker linked clean. But the codegen path for action /
  service interfaces emits MORE FFI symbols than talker exercises
  — talker only uses pub/sub, not action goal/feedback/result or
  service request/response paths. Action / service codegen pulls
  the full FFI surface, which is where `nros_cpp_publish_raw` lives.
- **Phase 152 is the highest-ROI remaining phase.** Single
  link-order or codegen-template fix knocks out 50 of 136
  remaining test failures.
- **Class B (dds_api) folded.** Both classes hit the same root
  cause (codegen-FFI archive vs nros-cpp staticlib link order);
  fixing Phase 152 closes both.
