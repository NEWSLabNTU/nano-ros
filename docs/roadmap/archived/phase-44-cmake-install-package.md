# Phase 44 — CMake Install Package

## Status: Complete

44.1–44.11 all complete.

## Background

The C API's CMake integration originally used `NANO_ROS_ROOT` to locate 7
artifacts via hardcoded repo-relative paths spread across 4 CMake modules.

**Problems solved by 44.1–44.5:**

1. **Only worked in the dev repo** — end-users couldn't install nros-c to a
   system prefix and use `find_package(NanoRos)` from arbitrary projects
2. **Fragile** — hardcoded paths broke if the repo structure changed
3. **Stale codegen library** — `just build` didn't rebuild the C codegen
   library when templates changed
4. **Inconsistent naming** — the codegen crate was `nano-ros-codegen-c` while
   all other crates use the `nros-` prefix

**Remaining problems (44.6–44.10):**

5. **Manual install script** — `install-local` is a shell script with many `cp`
   commands; no standard `--prefix`/`--destdir` interface for package
   maintainers
6. **Single RMW variant** — only `rmw-zenoh` is installed; XRCE examples use a
   `NROS_C_LIBRARY` override hack
7. **Unnecessary C wrapper** — the codegen tool is a Rust function wrapped in a
   C staticlib + C header + C `main()`, then compiled by CMake's `try_compile`
   at configure time. A Rust binary would eliminate all of this

### Goals

1. ~~Config-mode CMake package~~ ✓ (44.3)
2. ~~Pseudo-install directory~~ ✓ (44.2)
3. ~~Fresh build chain~~ ✓ (44.2)
4. ~~Rename codegen crate~~ ✓ (44.1)
5. ~~Delete legacy Find modules~~ ✓ (44.5)
6. **xtask install command** — `cargo xtask install --prefix /usr` for package
   maintainers (44.6, 44.9)
7. **Multi-RMW install** — `libnros_c_zenoh.a` and `libnros_c_xrce.a` side by
   side, selected via `NANO_ROS_RMW` CMake variable (44.8)
8. **Codegen as native binary** — `nros-codegen` Rust binary replaces
   staticlib + C wrapper + `try_compile` (44.7)

### Non-Goals

- Making the Zephyr C examples use the CMake package (they use west/CMake with
  direct source inclusion — a different build paradigm)
- Publishing the CMake package to any package registry
- Cross-compiling nros-c via CMake (Cargo handles this)
- Platform variants (Zephyr, bare-metal, FreeRTOS) — these will be dedicated
  packages, like Rust board crates. This phase covers native/posix only.

## Completed Sub-phases

### 44.1 — Rename `nano-ros-codegen-c` to `nros-codegen-c` ✓

Renamed the crate, library, header, and all references for naming consistency.

### 44.2 — Create pseudo-install layout and `install-local` recipe ✓

`build/install/` populated by `just install-local` with standard CMake layout.

### 44.3 — Write config-mode CMake package ✓

Three CMake files: `NanoRosConfig.cmake`, `NanoRosCTargets.cmake`,
`NanoRosGenerateInterfaces.cmake`. Entry point: `find_package(NanoRos CONFIG)`.

### 44.4 — Migrate C examples to config-mode ✓

All 10 native C example CMakeLists.txt files use `find_package(NanoRos CONFIG)`.
Build scripts pass `-DNanoRos_DIR=...` instead of hardcoded auto-detection.

### 44.5 — Delete old Find modules + clean up ✓

Deleted `FindNanoRos.cmake`, `FindNanoRosCodegen.cmake`, `FindNanoRosC.cmake`,
`nano_ros_generate_interfaces.cmake`, `nano_ros_cConfig.cmake.in`.

### 44.6 — Create xtask crate ✓

Created `packages/xtask/` with `install` subcommand replacing the shell script
in `install-local`. Supports `--prefix`, `--destdir`, `--rmw` options.

### 44.7 — Convert codegen C wrapper to Rust binary ✓

Replaced staticlib + C header + C `main()` + CMake `try_compile` with a native
Rust binary (`nros-codegen`). CMake now uses `find_program` instead of
`try_compile`.

### 44.8 — Multi-RMW variant install ✓

Install `libnros_c_zenoh.a` and `libnros_c_xrce.a` side by side, selected via
`NANO_ROS_RMW` CMake variable.

### 44.9 — Update justfile and documentation ✓

Simplified `install-local` to use `cargo xtask install`. Updated CLAUDE.md and
message-generation guide.

### 44.10 — Verification and package maintainer test ✓

Verified `just build`, `just test-c`, `just test-c-xrce`, `just quality` all
pass. System install test validated.

### 44.11 — Corrosion-based CMake build system ✓

Replaced xtask with standard CMake workflow using Corrosion (v0.6.1):
- Top-level `CMakeLists.txt` + per-package CMakeLists.txt
- `cmake -S . -B build && cmake --build build && cmake --install build`
- Single RMW variant per cmake invocation (multi-RMW = two builds to same prefix)
- Removed xtask crate entirely
- CMake config files co-located with their packages:
  - `packages/core/nros-c/cmake/` — NanoRosConfig.cmake, NanoRosCTargets.cmake
  - `packages/codegen/packages/nros-codegen-c/cmake/` — NanoRosGenerateInterfaces.cmake
- ARM toolchain file moved from `cmake/` to `scripts/qemu/`

## Install Layout

After installing (cmake or justfile):

```
$PREFIX/
├── bin/
│   └── nros-codegen                         # Codegen binary (Rust)
├── lib/
│   ├── libnros_c_zenoh.a                    # RMW=zenoh
│   ├── libnros_c_xrce.a                     # RMW=xrce
│   └── cmake/NanoRos/
│       ├── NanoRosConfig.cmake              # find_package entry point
│       ├── NanoRosCTargets.cmake            # NanoRos::NanoRos target
│       └── NanoRosGenerateInterfaces.cmake  # nano_ros_generate_interfaces()
├── include/nros/
│   ├── node.h, publisher.h, subscription.h, ...
│   └── platform/
│       ├── posix.h, zephyr.h, ...           # All headers shipped
└── share/nano-ros/
    └── interfaces/
        ├── std_msgs/msg/Int32.msg
        ├── builtin_interfaces/msg/Time.msg
        └── ...
```

## Design Decisions

### Corrosion-based CMake over xtask

Phase 44.6–44.10 used a Rust xtask binary to orchestrate the install. Phase
44.11 replaces it with standard CMake + Corrosion (v0.6.1), giving package
maintainers the familiar `cmake --build && cmake --install` workflow.

Corrosion builds one RMW variant per cmake invocation. Multi-RMW install is
achieved by running cmake twice to the same prefix (library names don't
collide: `libnros_c_zenoh.a` vs `libnros_c_xrce.a`).

### Codegen as native Rust binary

The current codegen flow compiles a C wrapper at CMake configure time:

```
lib.rs (#[no_mangle] FFI) → nros_codegen.h → codegen_main.c
  → CMake try_compile links all three → nros_codegen binary
```

This requires shipping a staticlib, a C header, and a C source file, plus
CMake must compile them at configure time. Converting to a Rust binary:

```
main.rs (clap CLI) → cargo build → nros-codegen binary
```

Eliminates: the staticlib crate-type, the C header, the C wrapper, the
`libexec/nano-ros/` directory, and the `try_compile` block in
`NanoRosGenerateInterfaces.cmake`. CMake just calls `find_program`.

### Multi-RMW via library naming (not link-time selection)

Each RMW backend produces a differently-linked `libnros_c.a`. Since static
libraries are self-contained archives, there's no way to select the backend at
link time — it must be chosen at build time.

The naming convention `libnros_c_{backend}.a` makes all variants coexist in a
single prefix. CMake's `NANO_ROS_RMW` variable (default: `zenoh`) selects the
right library. This replaces the `NROS_C_LIBRARY` path override hack.

ROS edition (humble/iron) does not produce a separate variant — it's
transparent at the C API level. The nros-c source code has zero `#[cfg]` guards
on ROS edition features; the differences exist only in the Rust middleware
layers below.

### Native platform only

This install layout targets `platform-posix` (desktop Linux/macOS). Embedded
platforms (Zephyr, bare-metal, FreeRTOS) use fundamentally different build
systems:

- **Zephyr**: west + CMake with direct source inclusion
- **Bare-metal**: Rust board crates with cargo as the build system
- **ESP-IDF**: idf.py with component integration

These will get dedicated packages (like the Rust board crates `nros-mps2-an385`,
`nros-esp32`, etc.), not variants within this install layout.

### CMake source files co-located with packages

Each CMake config file lives in the `cmake/` subdirectory of the package it
relates to. `NanoRosConfig.cmake` and `NanoRosCTargets.cmake` live in
`packages/core/nros-c/cmake/` (the library target), while
`NanoRosGenerateInterfaces.cmake` lives in
`packages/codegen/packages/nros-codegen-c/cmake/` (the codegen tool).
Each package's CMakeLists.txt installs its own cmake files to
`$PREFIX/lib/cmake/NanoRos/`.

## Package Maintainer Usage

Standard CMake workflow:
```bash
cmake -S . -B build -DNANO_ROS_RMW=zenoh -DCMAKE_BUILD_TYPE=Release
cmake --build build
cmake --install build --prefix /usr/local
```

Debian (both RMW variants):
```bash
for rmw in zenoh xrce; do
  cmake -S . -B "build-$rmw" -DNANO_ROS_RMW="$rmw" -DCMAKE_BUILD_TYPE=Release
  cmake --build "build-$rmw"
  cmake --install "build-$rmw" --prefix /usr --staging-prefix debian/nros/usr
done
```

End-user CMakeLists.txt:
```cmake
find_package(NanoRos REQUIRED CONFIG)

nano_ros_generate_interfaces(std_msgs "msg/Int32.msg" SKIP_INSTALL)

add_executable(my_app src/main.c)
target_link_libraries(my_app PRIVATE std_msgs__nano_ros_c NanoRos::NanoRos)
```

XRCE variant:
```bash
cmake -DNANO_ROS_RMW=xrce ..
```

## Freshness Chain

| Source change       | Rebuilds              | Mechanism                           |
|---------------------|-----------------------|-------------------------------------|
| Rust nros-c source  | `libnros_c_*.a`       | Cargo auto-recompiles (via Corrosion) |
| Rust codegen source | `nros-codegen` binary | Cargo auto-recompiles (via Corrosion) |
| Jinja templates     | `nros-codegen` binary | Cargo detects via `include_str!`    |
| nros-c headers      | `$PREFIX/include/`    | `cmake --install` copies            |
| Bundled .msg files  | `$PREFIX/share/`      | `cmake --install` copies            |
| CMake files         | `$PREFIX/lib/cmake/`  | `cmake --install` copies            |

## Future Work

- **Platform packages**: `nros-c-zephyr` (Zephyr module), `nros-c-freertos`,
  etc. — separate repos/crates
- **CPack packaging**: Create release tarballs via CMake's CPack
- **crates.io publishing**: Publish nros-c headers + CMake files as a crate
