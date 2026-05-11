# Phase 119.3 — CMake-Owned Header Dispatch

**Goal:** Simplify the three-write-path header emission from Phase 119.2. Source-tree generated header becomes a STUB; CMake (nros-cpp + nros-c) finds the right per-build header through a layered search.
**Status:** **DONE**. Build.rs reduced to two stable per-build paths (`$CARGO_TARGET_DIR/nros-{c,cpp}-generated/nros/` and `$CORROSION_BUILD_DIR/`). Source-tree headers are now stubs that emit `#error` when included without proper build-system setup. `just test-all` after 119.3: 716/720 pass (was 713/720 in 119.2; net +3 Zephyr XRCE CPP tests now pass because the per-build header is genuinely variant-exact).
**Priority:** Medium (simplification; not blocking tests).
**Depends on:** Phase 119.2 (variant-specific headers).

## Overview

Phase 119.2 made the generated `nros_cpp_config_generated.h` per-variant by writing the same content to three places: source tree (max-merged), `$CORROSION_BUILD_DIR` (cmake-corrosion drives posix install), and `$CARGO_TARGET_DIR/nros/` (Zephyr drives in-tree builds). Each consumer needed its own integration:

- `find_package(NanoRos)` users: `NanoRosCppTargets.cmake` prepends `include/nros_cpp_<variant>` to `INTERFACE_INCLUDE_DIRECTORIES`.
- Zephyr: `zephyr/CMakeLists.txt` prepends `${CMAKE_BINARY_DIR}/nros-rust` to `zephyr_include_directories`.
- Direct cargo: source-tree max-merged header as fallback.

Three write paths + three integration points = three places to keep in sync, three places a new RTOS port has to learn.

This phase consolidates: ONE write path, ONE consumer-facing function, every build system calls it.

## Architecture

### Single write path

`nros-cpp/build.rs` and `nros-c/build.rs` write the generated header to exactly one location per cargo invocation:

```
$CARGO_TARGET_DIR/nros-cpp-generated/<variant_slug>/nros/nros_cpp_config_generated.h
```

`<variant_slug>` is the sorted, underscore-joined list of all active features for nros-cpp's build, e.g. `platform-posix_rmw-zenoh_ros-humble_std`. Computed deterministically from `CARGO_FEATURE_*` env vars. New features automatically get their own slug — no per-variant CMake list to maintain.

Source-tree `include/nros/nros_cpp_config_generated.h` becomes a STUB that emits `#error` with a message pointing at the new include-path requirement. IDE and `cargo doc` users see a clear message; runtime code never includes the stub because the build system supplies a real header dir first.

### Public CMake function

```cmake
# packages/core/nros-cpp/cmake/NanoRosCpp.cmake  (exported via find_package)

# Wire <TARGET> against the right nros-cpp variant. Caller specifies
# RMW + PLATFORM (or inherits from cache). Function:
#   1. Computes variant slug.
#   2. Resolves the header path: install layout first, then in-tree
#      CARGO_TARGET_DIR layout.
#   3. Adds the variant header dir to TARGET's include directories.
#   4. Links NanoRos::NanoRosCpp (which handles the static lib + shared
#      headers via its own INTERFACE_INCLUDE_DIRECTORIES).
function(nros_cpp_setup TARGET)
    cmake_parse_arguments(NCS "" "RMW;PLATFORM" "EXTRA_FEATURES" ${ARGN})
    ...
endfunction()
```

Sibling: `nros_c_setup(TARGET ...)` for the C API.

### Per-build-system integration

- **find_package(NanoRos) consumers:** `NanoRosCppTargets.cmake` keeps its current logic (variant subdir on include path). Users who prefer the explicit setup call use `nros_cpp_setup()`; users who just `target_link_libraries(... NanoRos::NanoRosCpp)` still work.

- **Zephyr:** `zephyr/CMakeLists.txt` replaces its hardcoded `zephyr_include_directories(${CMAKE_BINARY_DIR}/nros-rust)` block with a call to `nros_cpp_setup()` (or its lower-level helpers, since Zephyr targets aren't standalone executables). Concretely Zephyr's `nros_cargo_build` macro learns to invoke the dispatch logic.

- **Future RTOS (PlatformIO, ESP-IDF, Arduino):** call `nros_cpp_setup()`. No changes inside nros-cpp.

- **Direct cargo:** writes to `$CARGO_TARGET_DIR/nros-cpp-generated/<slug>/`. Stub source-tree header errors with a message instructing the user to add `-I$CARGO_TARGET_DIR/nros-cpp-generated/<slug>` to their compile flags.

## Work Items

### 119.3.1 — Single write path in build.rs

- **Files:** `packages/core/nros-cpp/build.rs`, `packages/core/nros-c/build.rs`, `packages/core/nros-sizes-build/src/lib.rs`.
- Derive `variant_slug` from `CARGO_FEATURE_*` env (sorted, dash-preserving, underscore-joined).
- Compute target dir: honour `CARGO_TARGET_DIR` env; else use the OUT_DIR-walking heuristic from `nros-sizes-build::cargo_target_dir`.
- Write header to `<target_dir>/nros-cpp-generated/<slug>/nros/nros_cpp_config_generated.h`. Same for nros-c with `nros-c-generated`.
- Remove source-tree write (the max-merge from 119.1) and the CORROSION_BUILD_DIR + CARGO_TARGET_DIR per-build writes from 119.2.
- Replace committed source-tree headers with stub:

  ```c
  /* Auto-generated stub — overridden per-build by nros_cpp_setup() etc. */
  #ifndef NROS_CPP_CONFIG_GENERATED_H
  #define NROS_CPP_CONFIG_GENERATED_H
  #error "nros_cpp_config_generated.h must be supplied by the build system. \
          Call nros_cpp_setup(<target>) from CMake or add \
          -I$CARGO_TARGET_DIR/nros-cpp-generated/<variant_slug> to compile flags."
  #endif
  ```

### 119.3.2 — `nros_cpp_setup()` and `nros_c_setup()` CMake functions

- **Files:** new `packages/core/nros-cpp/cmake/NanoRosCpp.cmake`, `packages/core/nros-c/cmake/NanoRosC.cmake`. Update `NanoRosConfig.cmake` to include them.
- Function resolves the header dir from (a) the install prefix, (b) `${CMAKE_BINARY_DIR}/nros-cpp-generated/<slug>`, (c) `$ENV{CARGO_TARGET_DIR}/nros-cpp-generated/<slug>`. First hit wins.
- Adds dir to `target_include_directories(${TARGET} PRIVATE ...)`.
- Links `NanoRos::NanoRosCpp` (which carries shared headers + static lib).

### 119.3.3 — Update Zephyr CMakeLists

- **Files:** `zephyr/CMakeLists.txt`, `zephyr/cmake/nros_cargo_build.cmake`.
- Drop hardcoded `zephyr_include_directories(${CMAKE_BINARY_DIR}/nros-rust)` for both `CONFIG_NROS_C_API` and `CONFIG_NROS_CPP_API` blocks.
- After running `nros_cargo_build`, call `nros_cpp_setup()` to attach the right include path to the Zephyr `app` target.

### 119.3.4 — Update install + variant subdir handling

- **Files:** `packages/core/nros-cpp/CMakeLists.txt`, `packages/core/nros-c/CMakeLists.txt`, `cmake/...Targets.cmake`.
- Install rule sources the per-build header from `${CARGO_TARGET_DIR}/nros-cpp-generated/<slug>/...` (already determined by cmake; can `file(GLOB ...)`).
- Drop the previous CORROSION_BUILD_DIR-specific install rule.

### 119.3.5 — Tests

- `just install-local` succeeds; each variant's installed header in `include/nros_cpp_<variant>/nros/...` carries that variant's EXACT sizes.
- `just build-test-fixtures` + `cargo nextest run -E 'test(test_native_talker_listener_communication) + test(test_zephyr_cpp_action_server_to_client_e2e)'` all pass.
- `just test-all`: 713+/720, no new regressions vs 119.2.

### 119.3.6 — Documentation

- `packages/core/nros-cpp/docs/configuration.md` documents `nros_cpp_setup()`.
- `book/src/user-guide/...` adds a section on the variant header model.
- Stub header's `#error` message points to the docs.

## Acceptance

- [ ] 119.3.1 lands; build.rs writes exactly ONE per-build header.
- [ ] 119.3.2 lands; `nros_cpp_setup()` callable from any CMake context.
- [ ] 119.3.3 lands; Zephyr CMakeLists has no hardcoded nros-cpp paths.
- [ ] 119.3.4 lands; install layout uses the new per-build path.
- [ ] 119.3.5 passes; `just test-all` matches 119.2 outcome (713 pass).
- [ ] 119.3.6 lands; docs updated.

## Notes

### Why drop the source-tree write?

The committed source-tree header is the source of the entire phase 119 problem: it's a SHARED file that every cmake-variant write races on. Removing it removes the race condition by construction. The cost is that IDEs lose the canonical header location until they read the new include path from `compile_commands.json` (which CMake generates if `CMAKE_EXPORT_COMPILE_COMMANDS=ON`). Most modern C++ IDEs do this automatically.

### Why variant_slug from full feature set?

Phase 119.2's variant subdir uses only `<rmw>_<platform>`, ignoring features like `param-services` and `lifecycle-services` that ALSO affect Executor size. With `<slug>=<sorted feature list>`, every feature combination gets its own dir — no aliasing, no surprise size mismatches.

### Backward compatibility

`find_package(NanoRos)` continues to work for existing examples via `NanoRosCppTargets.cmake`. The new `nros_cpp_setup()` is an additional convenience, not a replacement. Existing CMakeLists.txt files don't need changes unless they were doing manual nros-cpp dispatch (Zephyr is the only one).

### Open question — direct cargo workflow

Power users who run `cargo build -p nros-cpp --features=... --target=...` and integrate the output into their own non-CMake build system will need to add `-I$CARGO_TARGET_DIR/nros-cpp-generated/<slug>` manually. This is documented but not auto-magical. If a major user (e.g. PX4) needs auto-detect, a follow-up phase can ship a helper script (`nros-cpp-include-flags`) that prints the right `-I` flag.
