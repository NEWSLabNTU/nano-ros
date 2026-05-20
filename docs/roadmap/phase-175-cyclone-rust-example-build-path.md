# Phase 175 - Cyclone DDS build path for pure-Rust examples

**Goal.** Give the pure-cargo Rust examples a way to actually link the
Cyclone DDS RMW backend. Phase 169 retired the pure-Rust dust-dds
backend and made Cyclone the canonical DDS RMW; the examples were
migrated from the deleted `nros-rmw-dds` crate onto
`nros-rmw-cyclonedds-sys`, but Cyclone cannot be linked by a plain
`cargo build` on any target. This phase designs and lands the build
path(s) that make `--features rmw-cyclonedds` link end-to-end.

**Status.** Deferred / not started. The `rmw-cyclonedds` feature is
defined in every migrated example's `Cargo.toml` but is intentionally
NOT exercised by any fixture matrix; all pure-cargo fixture matrices
build `zenoh` (+ `xrce` on native) only.

**Priority.** P2. Does not block `just ci` / `just test-all` (DDS is a
non-default example feature). Blocks DDS coverage for Rust examples.

**Depends on.**

- Phase 117: `nros-rmw-cyclonedds` C++ CMake backend + `NANO_ROS_RMW=cyclonedds`.
- Phase 11W: Cyclone DDS on Zephyr native_sim (the only working
  embedded Cyclone path; still in progress on `main`).
- Phase 169: dust-dds retirement; Cyclone as canonical DDS RMW.

## Background — why pure-cargo can't link Cyclone

`nros-rmw-cyclonedds-sys` is an `rlib`-only shim. Its `register()`
declares `extern "C" fn nros_rmw_cyclonedds_register()` and calls it;
that symbol is **defined in C++** in `packages/dds/nros-rmw-cyclonedds/`
(`src/vtable.cpp`), compiled by a CMake project that
`find_package(CycloneDDS)` and links `CycloneDDS::ddsc`. A plain
`cargo build --features rmw-cyclonedds` of an example only pulls in the
Rust shim — the C++ backend is never compiled or linked — so the link
fails with:

```
rust-lld: error: undefined symbol: nros_rmw_cyclonedds_register
```

This is confirmed on **native/posix as well as embedded** — it is not a
cross-compilation quirk; it is structural. dust-dds had no such problem
because it was pure Rust and linked entirely inside cargo.

The backend links today only when the build is **CMake-driven**:

- Native C / C++ examples: root `CMakeLists.txt` `NANO_ROS_RMW=cyclonedds`
  branch `add_subdirectory(packages/dds/nros-rmw-cyclonedds)` and links
  the static lib into `NanoRos` with `--whole-archive`.
- Zephyr Rust/C/C++: `zephyr/CMakeLists.txt` compiles the Cyclone DDS
  sources + the `nros-rmw-cyclonedds` C++ glue directly into the app
  library, against Zephyr's POSIX layer (Phase 11W).

## Work items

### 175.A — Native Rust example Cyclone CMake/Corrosion path

**Achievable build-glue.** Cyclone builds for Linux (`just cyclonedds
setup` installs it under `build/install/`). Give the native Rust
examples a CMakeLists.txt that:

- pulls in the Cyclone C++ backend (`add_subdirectory(packages/dds/nros-rmw-cyclonedds)`
  with `CMAKE_PREFIX_PATH` → the Cyclone install),
- imports the Rust example bin via `corrosion_import_crate(... FEATURES rmw-cyclonedds NO_DEFAULT_FEATURES)`,
- links the backend into the bin with
  `corrosion_link_libraries(<bin> -Wl,--whole-archive nros_rmw_cyclonedds -Wl,--no-whole-archive CycloneDDS::ddsc)`
  so `nros_rmw_cyclonedds_register` resolves and its
  `.linkm2_RMW_INIT_ENTRIES` section entry survives dead-strip.

**Files** (new): `examples/native/rust/<ex>/CMakeLists.txt`; a
`just native build-fixtures` arm that drives the cyclone variant via
CMake instead of `cargo build`.

**Acceptance:** native Rust talker/listener build + boot publishing on
Cyclone, interop-tested against the C/C++ Cyclone examples.

### 175.B — FreeRTOS / ThreadX Cyclone (ddsrt RTOS port)

**Not build-glue — a port.** Cyclone DDS abstracts its OS dependencies
(sockets, threads, time, sync) behind `ddsrt`. There is no FreeRTOS or
ThreadX `ddsrt` port. Standing one up is a weeks-scale effort
comparable to the Zephyr Cyclone bring-up (Phase 11W, still ongoing on
`main`), and likely a research-grade undertaking on bare-metal
thumbv7m / riscv64 where there is no hosted socket stack.

**Files** (new, large): a `ddsrt` port per RTOS; the embedded Cyclone
link wiring once a port exists.

**Acceptance:** out of scope for an estimate until 175.A lands and the
embedded networking story (smoltcp/NetX/lwIP ↔ ddsrt sockets) is
scoped on its own.

## Notes

- The migrated examples keep `rmw-cyclonedds = ["dep:nros-rmw-cyclonedds-sys"]`
  in `Cargo.toml` so the manifest resolves and the intent is recorded;
  the feature is simply never built by the fixture matrices until 175.A.
- Fixture recipes already reverted to zenoh-only on the pure-cargo
  paths: `just/{native,freertos,threadx-riscv64,threadx-linux,nuttx}.just`.
- Do NOT re-introduce a `for rmw in ... cyclonedds` / `... dds` arm into
  those pure-cargo loops without first landing 175.A.
