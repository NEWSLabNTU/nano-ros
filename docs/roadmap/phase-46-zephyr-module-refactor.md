# Phase 46 — Zephyr Module Refactor

## Status: Not Started

## Background

The current Zephyr integration has grown organically through several phases and
has usability and architectural gaps compared to Zephyr module best practices.

**Current state:**

- `zpico-zephyr` serves as both zenoh-pico platform glue AND the nros user API
  (via `bsp_zephyr.c`), conflating two concerns
- `zpico-zephyr/zephyr/module.yml` registers a module named
  `nano-ros-bsp-zephyr`, but examples don't actually use it — they manually
  include sources with hardcoded relative paths
- Buffer constants (`ZPICO_MAX_PUBLISHERS`, etc.) use `#ifndef` fallback
  defaults in `zenoh_shim.c` because Zephyr CMake doesn't pass `-D` flags
- Kconfig entries exist (`CONFIG_NANO_ROS_MAX_PUBLISHERS`) but are disconnected
  from the actual constants the C shim uses
- C Zephyr examples use the limited `bsp_zephyr.c` API instead of the full
  `nros-c` API
- Rust Zephyr examples manually add `target_sources` for shim and BSP in every
  `CMakeLists.txt` (~36 lines of boilerplate each)
- No Kconfig→Cargo env var bridge — Rust builds ignore Kconfig values

**Problems:**

1. **40-line boilerplate** per example `CMakeLists.txt` with fragile relative paths
2. **Two configuration systems** (env vars for Cargo, Kconfig for Zephyr) with
   no bridge between them
3. **C users limited to bsp_zephyr.c** — a thin wrapper around zenoh_shim,
   missing executor, lifecycle, actions, services, parameters, codegen
4. **No automatic library linking** — users must know internal package structure
5. **Inconsistent naming** — `CONFIG_NANO_ROS_*` in Kconfig vs `ZPICO_*` in env
   vars for the same concepts
6. **Stale Kconfig prefix** — zpico-zephyr Kconfig still uses the old
   `NANO_ROS_` prefix (`CONFIG_NANO_ROS_BSP`, `CONFIG_NANO_ROS_DOMAIN_ID`,
   etc.) instead of the project-wide `NROS_` prefix

### Goals

1. **Single Zephyr module** at repo root — `zephyr/module.yml`, `zephyr/Kconfig`,
   `zephyr/CMakeLists.txt` — replaces `zpico-zephyr/zephyr/module.yml`
2. **Kconfig as single source of truth** — all tunable constants configured in
   `prj.conf`, propagated to both C (`-D` flags) and Rust (Cargo env vars)
3. **C API path** — `CONFIG_NROS_C_API=y` cross-compiles `nros-c` via Cargo and
   links it automatically (full nros C API on Zephyr)
4. **Rust API path** — `CONFIG_NROS_RUST_API=y` bridges Kconfig→env vars so
   `rust_cargo_application()` picks up the right constants
5. **3-line example CMakeLists** — no manual `target_sources` or relative paths
6. **Reposition zpico-zephyr** as pure zenoh-pico platform support (network
   wait, session init) — remove the nros BSP API layer
7. **Revise all Zephyr examples** to use the refactored module

### Non-Goals

- Making zpico-zephyr a standalone Zephyr module (the root module handles
  registration)
- Publishing the Zephyr module to any registry
- Supporting Zephyr versions other than 3.7.x (current west.yml pin)
- Pre-built binary distribution of `libnros_c.a` for Zephyr targets
- Changing the `west.yml` manifest structure or workspace layout
- Zephyr C codegen integration (`nano_ros_generate_interfaces()` for Zephyr
  CMake) — future phase

---

## Architecture

### Layer Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                     User Application                            │
│                                                                 │
│   C user:                          Rust user:                   │
│   #include <nros/node.h>           use nros::EmbeddedExecutor;  │
│   nano_ros_init(...);              let exec = ...open(&cfg)?;   │
├─────────────────────────────────────────────────────────────────┤
│   nros-c (libnros_c.a)        │   nros crate                   │
│   Built by module CMake       │   Built by rust_cargo_app()     │
│   via nros_cargo_build()      │   in user's CMakeLists.txt      │
├───────────────────────────────┴─────────────────────────────────┤
│                   zenoh_shim.c (from zpico-sys)                 │
│                   Compiled by module CMake with Kconfig -D flags│
├─────────────────────────────────────────────────────────────────┤
│              zpico-zephyr (platform support)                    │
│              Network wait, Zephyr logging, session helpers      │
├─────────────────────────────────────────────────────────────────┤
│              zenoh-pico (existing Zephyr module)                │
│              CONFIG_ZENOH_PICO=y                                │
├─────────────────────────────────────────────────────────────────┤
│                      Zephyr RTOS kernel                         │
└─────────────────────────────────────────────────────────────────┘
```

### File Structure (new/changed files only)

```
nano-ros/
├── zephyr/                              # NEW: root Zephyr module
│   ├── module.yml                       # name: nros
│   ├── CMakeLists.txt                   # Module build logic (shim + C API + Rust bridge)
│   ├── Kconfig                          # All nros Kconfig options
│   └── cmake/
│       └── nros_cargo_build.cmake       # Target detection + Cargo invocation helpers
│
├── packages/zpico/zpico-zephyr/         # REFACTORED: zenoh-pico platform support only
│   ├── src/zpico_zephyr.c              # RENAMED from bsp_zephyr.c; nros API removed
│   ├── include/zpico_zephyr.h          # RENAMED from nano_ros_bsp_zephyr.h; platform API only
│   ├── Kconfig                          # SLIMMED: only zpico-zephyr-specific options
│   ├── CMakeLists.txt                   # KEPT: conditional zephyr_library() for standalone use
│   └── zephyr/module.yml               # REMOVED: root module handles registration
│
├── examples/zephyr/
│   ├── c/zenoh/talker/                  # REVISED
│   │   ├── CMakeLists.txt              # 3 lines
│   │   ├── prj.conf                    # CONFIG_NROS=y, CONFIG_NROS_C_API=y
│   │   └── src/main.c                  # Uses nros-c API
│   ├── c/zenoh/listener/               # REVISED (same pattern)
│   ├── rust/zenoh/talker/              # REVISED
│   │   ├── CMakeLists.txt              # 3 lines + rust_cargo_application()
│   │   ├── prj.conf                    # CONFIG_NROS=y, CONFIG_NROS_RUST_API=y
│   │   ├── Cargo.toml                  # unchanged
│   │   └── src/lib.rs                  # unchanged
│   └── rust/zenoh/*/                   # All 6 Rust examples revised
```

### Configuration Flow

```
prj.conf (single source of truth)
    │
    ├─→ Kconfig → autoconf.h → C application code (#ifdef CONFIG_NROS_...)
    │
    ├─→ Kconfig → zephyr_compile_definitions() → zenoh_shim.c (-DZPICO_MAX_*=N)
    │
    ├─→ Kconfig → nros_set_cargo_env_from_kconfig()
    │       │
    │       ├─→ nros_cargo_build() → build.rs → libnros_c.a  (C path)
    │       │
    │       └─→ rust_cargo_application() → build.rs → nros crate  (Rust path)
    │
    └─→ Non-Zephyr builds: env vars or #ifndef defaults still work
```

### Kconfig → Env Var Mapping

| Kconfig | Env Var | Consumer |
|---------|---------|----------|
| `CONFIG_NROS_MAX_PUBLISHERS` | `ZPICO_MAX_PUBLISHERS` | zpico-sys build.rs |
| `CONFIG_NROS_MAX_SUBSCRIBERS` | `ZPICO_MAX_SUBSCRIBERS` | zpico-sys build.rs |
| `CONFIG_NROS_MAX_QUERYABLES` | `ZPICO_MAX_QUERYABLES` | zpico-sys build.rs |
| `CONFIG_NROS_MAX_LIVELINESS` | `ZPICO_MAX_LIVELINESS` | zpico-sys build.rs |
| `CONFIG_NROS_SUBSCRIBER_BUFFER_SIZE` | `ZPICO_SUBSCRIBER_BUFFER_SIZE` | nros-rmw-zenoh build.rs |
| `CONFIG_NROS_SERVICE_BUFFER_SIZE` | `ZPICO_SERVICE_BUFFER_SIZE` | nros-rmw-zenoh build.rs |
| `CONFIG_NROS_FRAG_MAX_SIZE` | `ZPICO_FRAG_MAX_SIZE` | zpico-sys build.rs |
| `CONFIG_NROS_BATCH_UNICAST_SIZE` | `ZPICO_BATCH_UNICAST_SIZE` | zpico-sys build.rs |
| `CONFIG_NROS_C_MAX_EXECUTORS` | `NROS_C_MAX_EXECUTORS` | nros-c build.rs |
| `CONFIG_NROS_C_MAX_NODES` | `NROS_C_MAX_NODES` | nros-c build.rs |
| `CONFIG_NROS_C_MAX_SUBSCRIPTIONS` | `NROS_C_MAX_SUBSCRIPTIONS` | nros-c build.rs |
| `CONFIG_NROS_C_MAX_TIMERS` | `NROS_C_MAX_TIMERS` | nros-c build.rs |
| `CONFIG_NROS_C_MAX_SERVICES` | `NROS_C_MAX_SERVICES` | nros-c build.rs |
| `CONFIG_NROS_C_MAX_CLIENTS` | `NROS_C_MAX_CLIENTS` | nros-c build.rs |

### Design Decisions

**Single module at repo root, not in zpico-zephyr.** The nros Zephyr module
spans multiple packages (zpico-sys, zpico-zephyr, nros-c, nros-rmw-zenoh). It
doesn't belong inside any single package. The root `zephyr/` directory makes
the module visible at the top level, matching where `west.yml` already lives.

**Workspace Cargo invocation.** The module's CMake invokes
`cargo build -p nros-c --manifest-path ${MODULE_DIR}/Cargo.toml` using the
existing Cargo workspace. This avoids duplicating path dependencies, shares
`Cargo.lock` for reproducibility, and reuses `target/` to avoid redundant
compilation of shared deps. The `--target-dir` flag directs output into the
Zephyr build directory to avoid locking conflicts with concurrent `cargo build`.

**Custom `nros_cargo_build()` over `zephyr_add_rust_library`.** The third-party
`zephyr_add_rust_library` has limited arch support and uses
`--allow-multiple-definition` to mask linker issues. A custom function gives us
precise target mapping, proper env var bridging, and no hidden linker hacks.

**C API path uses nros-c, not bsp_zephyr.c.** The old `bsp_zephyr.c` API is a
thin wrapper around zenoh_shim that only supports pub/sub with raw bytes. The
nros-c API provides executor, lifecycle, actions, services, parameters, codegen
integration, and CDR serialization — a complete ROS 2 client library. Users
upgrading to nros-c get a vastly richer API.

**zpico-sys build.rs skips zenoh_shim.c on platform-zephyr.** When
`platform-zephyr` feature is active, the Zephyr module's CMake compiles
`zenoh_shim.c` with Kconfig-derived `-D` flags. The Cargo build.rs must skip
compiling the same file to avoid duplicate symbol errors. The `#ifndef`
fallback defaults in `zenoh_shim.c` remain for non-Zephyr build paths.

**Kconfig `choice` for API selection.** `NROS_C_API` and `NROS_RUST_API` are
mutually exclusive via Kconfig `choice`. A single Zephyr image links either
`libnros_c.a` (for C apps) or lets the user's Cargo build pull in the `nros`
crate (for Rust apps). Both paths share the same zenoh_shim + platform layer.

**`NROS_` Kconfig prefix.** The old zpico-zephyr Kconfig used the
`NANO_ROS_` prefix (`CONFIG_NANO_ROS_BSP`, `CONFIG_NANO_ROS_DOMAIN_ID`, etc.).
This phase adopts the project-wide `NROS_` prefix (`CONFIG_NROS`,
`CONFIG_NROS_DOMAIN_ID`, etc.), consistent with the crate rename completed in
Phase 33 and the constant rename completed in Phase 45.

---

## Sub-phases

### 46.1 — Create root Zephyr module structure

Create `zephyr/` directory at repo root with module descriptor and skeleton
build files.

- [ ] Create `zephyr/module.yml` with `name: nros`
- [ ] Create `zephyr/Kconfig` with full config hierarchy using `NROS_` prefix
  (API choice, transport tuning, buffer sizing, C API executor limits).
  This replaces the old `NANO_ROS_*` Kconfig prefix from zpico-zephyr
- [ ] Create `zephyr/CMakeLists.txt` skeleton (conditional on `CONFIG_NROS`)
- [ ] Create `zephyr/cmake/nros_cargo_build.cmake` with:
  - `nros_detect_rust_target()` — maps Zephyr `CONFIG_*` to Rust target triple
  - `nros_set_cargo_env_from_kconfig()` — bridges Kconfig→env vars
  - `nros_cargo_build()` — invokes Cargo for cross-compilation
- [ ] Remove `packages/zpico/zpico-zephyr/zephyr/module.yml`
- [ ] Update `west.yml` `self:` section if needed (module discovery uses
  `zephyr/module.yml` in the manifest repo automatically)
- [ ] Verify: `west build -t menuconfig` shows NROS menu under the nros module

### 46.2 — Wire zenoh_shim.c compilation into module CMake

Move zenoh_shim.c compilation from per-example boilerplate into the module.

- [ ] Add `zephyr_library_sources(zenoh_shim.c)` to `zephyr/CMakeLists.txt`
- [ ] Add `zephyr_compile_definitions()` mapping all Kconfig buffer constants to
  `-DZPICO_MAX_*` flags
- [ ] Add `zephyr_include_directories()` for `zpico-sys/c/include`
- [ ] Verify: `zenoh_shim.c` compiles with Kconfig values (not `#ifndef`
  fallbacks) when building a Zephyr example

### 46.3 — Wire zpico-zephyr platform support into module CMake

Add zpico-zephyr platform initialization to the module's shared layer.

- [ ] Add `zephyr_library_sources(zpico_zephyr.c)` to `zephyr/CMakeLists.txt`
- [ ] Add `zephyr_include_directories()` for `zpico-zephyr/include`
- [ ] Verify: platform init functions available to both C and Rust paths

### 46.4 — Refactor zpico-zephyr to platform-only

Remove the nros BSP API from zpico-zephyr, keeping only zenoh-pico platform
support.

- [ ] Rename `bsp_zephyr.c` → `zpico_zephyr.c`
- [ ] Rename `nano_ros_bsp_zephyr.h` → `zpico_zephyr.h`
- [ ] Remove nros-specific API functions:
  - `nano_ros_bsp_create_node()`, `nano_ros_bsp_create_node_with_domain()`
  - `nano_ros_bsp_create_publisher()`, `nano_ros_bsp_publish()`,
    `nano_ros_bsp_destroy_publisher()`
  - `nano_ros_bsp_create_subscriber()`, `nano_ros_bsp_destroy_subscriber()`
  - `nano_ros_bsp_spin_once()`, `nano_ros_bsp_spin()`
  - `nano_ros_bsp_build_keyexpr()`, `nano_ros_bsp_build_keyexpr_wildcard()`
  - `nano_ros_bsp_context_t`, `nros_node_t`, `nano_ros_publisher_t`,
    `nano_ros_subscriber_t` types
  - `NROS_BSP_*` error codes
- [ ] Keep platform-level functions:
  - `zpico_zephyr_wait_network(int timeout_ms)` — wait for `net_if_is_up()`
  - `zpico_zephyr_init_session(const char *locator)` — `zenoh_shim_init()` +
    `zenoh_shim_open()`
  - `zpico_zephyr_shutdown()` — `zenoh_shim_close()`
- [ ] Slim down `zpico-zephyr/Kconfig` to platform-only options (init delay,
  locator) using `NROS_` prefix (rename from old `NANO_ROS_*`), referenced
  by `zephyr/Kconfig` via `source`
- [ ] Update zpico-zephyr `CMakeLists.txt` for standalone use (when not used
  through the root module)
- [ ] Verify: zpico-zephyr builds without nros dependencies

### 46.5 — Implement Kconfig→Cargo env var bridge

Enable Kconfig values to flow into Cargo's build.rs for both build paths.

- [ ] Implement `nros_set_cargo_env_from_kconfig()` in
  `zephyr/cmake/nros_cargo_build.cmake`
- [ ] Map all Kconfig int/string values to their corresponding `ZPICO_*` /
  `NROS_*` env vars (see mapping table above)
- [ ] Ensure env vars are visible to both `nros_cargo_build()` (C path) and
  `rust_cargo_application()` (Rust path)
- [ ] Verify: build a Rust example with non-default Kconfig values, confirm
  `build.rs` picks them up (check generated constants match `prj.conf`)

### 46.6 — Implement nros_cargo_build() for C API path

Enable cross-compilation of `nros-c` from the Cargo workspace.

- [ ] Implement `nros_detect_rust_target()`:
  - `CONFIG_CPU_CORTEX_M3` → `thumbv7m-none-eabi`
  - `CONFIG_CPU_CORTEX_M4`/`M7` → `thumbv7em-none-eabi[hf]`
  - `CONFIG_SOC_SERIES_ESP32C3` → `riscv32imc-unknown-none-elf`
  - `CONFIG_BOARD_NATIVE_SIM` → `x86_64-unknown-linux-gnu`
- [ ] Implement `nros_cargo_build()`:
  - Invoke `cargo build -p nros-c` with `--manifest-path`, `--target`,
    `--target-dir ${CMAKE_BINARY_DIR}/nros-rust`, `--release`,
    `--no-default-features`, `--features "rmw-zenoh,platform-zephyr,ros-humble"`
  - Locate output `libnros_c.a`
  - Create imported CMake target `nros::nros-c`
- [ ] Wire into `zephyr/CMakeLists.txt`:
  - `target_link_libraries(app PUBLIC nros::nros-c)`
  - `zephyr_include_directories()` for `nros-c/include`
- [ ] Verify: `west build` with `CONFIG_NROS_C_API=y` produces a linkable
  binary for `native_sim`

### 46.7 — Guard zenoh_shim.c in zpico-sys build.rs

Prevent double-compilation of `zenoh_shim.c` when building for Zephyr.

- [ ] In `packages/zpico/zpico-sys/build.rs`, skip `cc::Build` for
  `zenoh_shim.c` when `platform-zephyr` feature is active
- [ ] Ensure non-Zephyr builds (posix, bare-metal) still compile `zenoh_shim.c`
  via build.rs as before
- [ ] Verify: no duplicate symbol errors when building Rust Zephyr examples
- [ ] Verify: `just quality` still passes (non-Zephyr builds unaffected)

### 46.8 — Revise Rust Zephyr examples

Update all 6 Rust Zephyr examples to use the module instead of manual paths.

- [ ] `examples/zephyr/rust/zenoh/talker/CMakeLists.txt` — reduce to:
  ```cmake
  cmake_minimum_required(VERSION 3.20.0)
  find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
  project(nros_zephyr_talker_rs)
  rust_cargo_application()
  ```
- [ ] Update `prj.conf` for all 6 examples:
  - Add `CONFIG_NROS=y` and `CONFIG_NROS_RUST_API=y`
  - Remove commented-out `CONFIG_NANO_ROS_BSP` lines
  - Replace any hardcoded buffer values with `CONFIG_NROS_*` equivalents
- [ ] Remove `zephyr_compile_definitions(Z_FEATURE_INTEREST=0 ...)` from
  examples — move to module CMake if still needed
- [ ] Repeat for: talker, listener, service-server, service-client,
  action-server, action-client
- [ ] Verify: `just build-zephyr` succeeds
- [ ] Verify: `just test-zephyr` passes

### 46.9 — Revise C Zephyr examples

Update C Zephyr examples to use nros-c API via the module.

- [ ] `examples/zephyr/c/zenoh/talker/CMakeLists.txt` — reduce to:
  ```cmake
  cmake_minimum_required(VERSION 3.20.0)
  find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
  project(nros_zephyr_talker_c)
  target_sources(app PRIVATE src/main.c)
  ```
- [ ] Update `prj.conf`:
  - Add `CONFIG_NROS=y` and `CONFIG_NROS_C_API=y`
  - Add `CONFIG_NROS_ZENOH_LOCATOR`, `CONFIG_NROS_DOMAIN_ID`, etc.
- [ ] Rewrite `src/main.c` to use nros-c API (`nano_ros_init`,
  `nano_ros_node_create`, `nano_ros_publisher_create`, etc.) instead of
  `bsp_zephyr.c` API
- [ ] Repeat for: talker, listener
- [ ] Verify: `just build-zephyr-c` succeeds
- [ ] Verify: `just test-zephyr` passes (C examples)

### 46.10 — Update justfile and test infrastructure

Update build recipes and test fixtures to work with the refactored module.

- [ ] Update `just build-zephyr` if build commands changed
- [ ] Update `just build-zephyr-c` if build commands changed
- [ ] Update `just test-zephyr` if test expectations changed
- [ ] Update test fixtures in `packages/testing/nros-tests/src/zephyr.rs` if
  binary paths or output patterns changed
- [ ] Update `scripts/zephyr/setup.sh` if any setup steps changed
- [ ] Verify: `just test-all` passes

### 46.11 — Documentation

Update all docs to reflect the refactored module architecture.

- [ ] Update `CLAUDE.md`:
  - Add `zephyr/` to workspace structure
  - Update zpico-zephyr description (platform support, not BSP)
  - Document Kconfig options and their mapping to env vars
  - Add Zephyr module usage instructions
- [ ] Update `docs/guides/zephyr-setup.md`:
  - Remove references to manual `target_sources` boilerplate
  - Document `CONFIG_NROS` / `CONFIG_NROS_C_API` / `CONFIG_NROS_RUST_API`
  - Add `prj.conf` reference for all Kconfig options
- [ ] Update `docs/guides/creating-examples.md`:
  - Revise Zephyr example section with new minimal CMakeLists.txt
  - Document C API vs Rust API path selection
- [ ] Mark phase 46 as Complete in this file
- [ ] Verify: all doc cross-references are valid

---

## Acceptance Criteria

### Functional

1. `west build -t menuconfig` shows the NROS menu with all options
2. C Zephyr talker builds and runs with `CONFIG_NROS_C_API=y` (native_sim)
3. Rust Zephyr talker builds and runs with `CONFIG_NROS_RUST_API=y` (native_sim)
4. Changing `CONFIG_NROS_MAX_PUBLISHERS=2` in `prj.conf` affects both C and
   Rust builds (verified by checking generated constants or link-time failure
   when exceeding the limit)
5. All 6 Rust Zephyr examples build and pass tests
6. Both C Zephyr examples build and pass tests
7. `just test-all` passes (no regressions)

### Structural

8. No example `CMakeLists.txt` contains `target_sources` for zenoh_shim.c or
   bsp_zephyr.c
9. No example `CMakeLists.txt` contains hardcoded relative paths to
   `packages/zpico/`
10. `zpico-zephyr` has zero nros-specific API (no `nano_ros_bsp_*` functions)
11. `zpico-zephyr/zephyr/module.yml` does not exist (root module handles it)
12. `zenoh_shim.c` is compiled exactly once per build (no duplicate symbols)

### Configuration

13. All Kconfig values in the mapping table propagate correctly to Cargo env
    vars (verified by building with non-default values)
14. Non-Zephyr builds (`just quality`) are unaffected — env vars and `#ifndef`
    defaults work as before
15. `prj.conf` is the only file users need to edit for configuration

---

## Risk Assessment

**Env var propagation timing.** CMake `set(ENV{...})` sets process-level env
vars during configure time. `rust_cargo_application()` spawns Cargo during
build time. If Cargo inherits the CMake process env (typical for
`add_custom_command`), this works. If not, we may need to generate a
`.cargo/config.toml` or pass env vars via the custom command's `COMMAND`
property. Mitigation: test early in 46.5.

**nros-c on native_sim.** `nros-c` has only been built for host (x86_64) and
bare-metal ARM targets. Building for `native_sim` (x86_64-unknown-linux-gnu)
with `platform-zephyr` is a new combination that may surface link issues.
Mitigation: validate in 46.6 before revising examples.

**Duplicate symbols.** Both the module CMake and zpico-sys build.rs want to
compile `zenoh_shim.c`. The feature gate in 46.7 must be correct, and both C
and Rust Zephyr paths must be tested. Mitigation: 46.7 is a prerequisite for
46.8 and 46.9.

**zephyr-lang-rust stability.** The `rust_cargo_application()` CMake function
tracks `main` branch. API changes could break the Rust path. Mitigation: pin
to a specific revision in `west.yml` after validation.
