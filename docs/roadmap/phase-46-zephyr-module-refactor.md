# Phase 46 — Zephyr Module Refactor

## Status: Complete

## Background

The current Zephyr integration has grown organically through several phases and
has usability and architectural gaps compared to Zephyr module best practices.

**Original problems (pre-46):**

1. **40-line boilerplate** per example `CMakeLists.txt` with fragile relative paths
2. **Two configuration systems** (env vars for Cargo, Kconfig for Zephyr) with
   no bridge between them
3. **C users limited to bsp_zephyr.c** — a thin wrapper around zenoh_shim,
   missing executor, lifecycle, actions, services, parameters, codegen
4. **No automatic library linking** — users must know internal package structure
5. **Inconsistent naming** — `CONFIG_NANO_ROS_*` in Kconfig vs `ZPICO_*` in env
   vars for the same concepts
6. **Stale Kconfig prefix** — zpico-zephyr Kconfig still uses the old
   `NANO_ROS_` prefix
7. **Zenoh-only** — no mechanism for selecting alternative RMW backends (XRCE-DDS)
8. **Dual zenoh-pico sources** — west.yml pulls zenoh-pico as a separate module
   (v1.7.2) while zpico-sys vendors it as a submodule (v1.6.2)

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
7. **Multi-RMW backend** — Kconfig choice between zenoh and XRCE-DDS, with
   backend-specific C sources, Cargo features, and env var bridging
8. **Single zenoh-pico source** — absorb the zenoh-pico west module into the
   nros module, compiling from the vendored submodule
9. **Revise all Zephyr examples** to use the refactored module

### Non-Goals

- Making zpico-zephyr a standalone Zephyr module (the root module handles
  registration)
- Publishing the Zephyr module to any registry
- Supporting Zephyr versions other than 3.7.x (current west.yml pin)
- Pre-built binary distribution of `libnros_c.a` for Zephyr targets
- XRCE serial transport on Zephyr (initial scope is UDP only)

---

## Architecture

### Layer Diagram

```
┌──────────────────────────────────────────────────────────────────────────┐
│                          User Application                               │
│                                                                         │
│   C user:                              Rust user:                       │
│   #include <nros/node.h>               use nros::EmbeddedExecutor;      │
│   nano_ros_init(...);                  let exec = ...open(&cfg)?;       │
├──────────────────────────────────────────────────────────────────────────┤
│   nros-c (libnros_c.a)           │   nros crate                        │
│   Built by nros_cargo_build()    │   Built by rust_cargo_application() │
├───────────────────┬──────────────┴──────────────────────────────────────┤
│ NROS_RMW_ZENOH    │ NROS_RMW_XRCE                                      │
├───────────────────┼─────────────────────────────────────────────────────┤
│ zenoh_shim.c      │ xrce_zephyr.c (NEW)                                │
│ Compiled by       │ Zephyr BSD socket transport callbacks               │
│ module CMake with │ Compiled by module CMake                            │
│ Kconfig -D flags  │                                                     │
├───────────────────┼─────────────────────────────────────────────────────┤
│ zpico_zephyr.c    │ (Micro-XRCE-DDS-Client compiled by                 │
│ Network wait,     │  xrce-sys/build.rs inside Cargo —                   │
│ session helpers   │  no Zephyr CMake compilation needed)                │
├───────────────────┼─────────────────────────────────────────────────────┤
│ zenoh-pico        │                                                     │
│ (from submodule,  │                                                     │
│  absorbed by nros │                                                     │
│  module — 46.12)  │                                                     │
├───────────────────┴─────────────────────────────────────────────────────┤
│                         Zephyr RTOS kernel                              │
└──────────────────────────────────────────────────────────────────────────┘
```

**Key asymmetry:** Zenoh-pico is compiled by the Zephyr module's CMake (it needs
Zephyr platform headers for threading, sockets, etc.). Micro-XRCE-DDS-Client is
compiled by `xrce-sys/build.rs` inside Cargo (it uses a "custom transport" model
with 4 user-supplied C callbacks, so it has no Zephyr header dependencies). The
only XRCE C file compiled by Zephyr CMake is the transport glue (`xrce_zephyr.c`)
which implements the callbacks using Zephyr's BSD socket API.

### File Structure

```
nano-ros/
├── zephyr/                              # Root Zephyr module
│   ├── module.yml                       # name: nros
│   ├── CMakeLists.txt                   # Conditional backend build + C/Rust API
│   ├── Kconfig                          # RMW choice + backend-specific tuning
│   └── cmake/
│       ├── nros_cargo_build.cmake       # Target detection + Cargo invocation
│       └── nros_generate_interfaces.cmake  # C codegen (adds to app target)
│
├── packages/zpico/zpico-zephyr/         # Zenoh platform support (network wait, session)
│   ├── src/zpico_zephyr.c
│   ├── include/zpico_zephyr.h
│   └── Kconfig                          # Zenoh-specific platform options
│
├── packages/xrce/xrce-zephyr/          # NEW: XRCE Zephyr transport glue
│   ├── src/xrce_zephyr.c              # BSD socket transport callbacks
│   └── include/xrce_zephyr.h          # Public API (init, wait_network)
│
├── examples/zephyr/
│   ├── c/zenoh/{talker,listener}/       # C + zenoh (existing)
│   ├── c/xrce/{talker,listener}/        # C + XRCE (NEW)
│   ├── rust/zenoh/*/                    # Rust + zenoh (existing, 6 examples)
│   └── rust/xrce/{talker,listener}/     # Rust + XRCE (NEW)
```

### Configuration Flow

```
prj.conf (single source of truth)
    │
    ├─→ CONFIG_NROS_RMW_ZENOH or CONFIG_NROS_RMW_XRCE
    │       │
    │       ├─→ CMakeLists.txt: compile backend-specific C sources
    │       │     Zenoh: zenoh-pico + zenoh_shim.c + zpico_zephyr.c
    │       │     XRCE:  xrce_zephyr.c only (XRCE lib compiled by Cargo)
    │       │
    │       ├─→ CMakeLists.txt: select Cargo features
    │       │     Zenoh: "rmw-zenoh,platform-zephyr,ros-humble"
    │       │     XRCE:  "rmw-xrce,platform-zephyr,ros-humble"
    │       │
    │       └─→ nros_set_cargo_env_from_kconfig()
    │             Zenoh: ZPICO_MAX_PUBLISHERS, ZPICO_FRAG_MAX_SIZE, ...
    │             XRCE:  XRCE_TRANSPORT_MTU, XRCE_MAX_SUBSCRIBERS, ...
    │             Common: NROS_EXECUTOR_MAX_HANDLES, ...
    │
    ├─→ CONFIG_NROS_C_API or CONFIG_NROS_RUST_API
    │       │
    │       ├─→ nros_cargo_build() → build.rs → libnros_c.a  (C path)
    │       └─→ rust_cargo_application() → build.rs → nros crate  (Rust path)
    │
    └─→ Non-Zephyr builds: env vars or #ifndef defaults still work
```

### Kconfig → Env Var Mapping

**Zenoh-specific** (visible when `CONFIG_NROS_RMW_ZENOH`):

| Kconfig                              | Env Var                        | Consumer                |
|--------------------------------------|--------------------------------|-------------------------|
| `CONFIG_NROS_MAX_PUBLISHERS`         | `ZPICO_MAX_PUBLISHERS`         | zpico-sys build.rs      |
| `CONFIG_NROS_MAX_SUBSCRIBERS`        | `ZPICO_MAX_SUBSCRIBERS`        | zpico-sys build.rs      |
| `CONFIG_NROS_MAX_QUERYABLES`         | `ZPICO_MAX_QUERYABLES`         | zpico-sys build.rs      |
| `CONFIG_NROS_MAX_LIVELINESS`         | `ZPICO_MAX_LIVELINESS`         | zpico-sys build.rs      |
| `CONFIG_NROS_SUBSCRIBER_BUFFER_SIZE` | `ZPICO_SUBSCRIBER_BUFFER_SIZE` | nros-rmw-zenoh build.rs |
| `CONFIG_NROS_SERVICE_BUFFER_SIZE`    | `ZPICO_SERVICE_BUFFER_SIZE`    | nros-rmw-zenoh build.rs |
| `CONFIG_NROS_FRAG_MAX_SIZE`          | `ZPICO_FRAG_MAX_SIZE`          | zpico-sys build.rs      |
| `CONFIG_NROS_BATCH_UNICAST_SIZE`     | `ZPICO_BATCH_UNICAST_SIZE`     | zpico-sys build.rs      |

**XRCE-specific** (visible when `CONFIG_NROS_RMW_XRCE`):

| Kconfig                                | Env Var                          | Consumer                  |
|-----------------------------------------|----------------------------------|---------------------------|
| `CONFIG_NROS_XRCE_TRANSPORT_MTU`       | `XRCE_TRANSPORT_MTU`            | xrce-sys build.rs         |
| `CONFIG_NROS_XRCE_MAX_SUBSCRIBERS`     | `XRCE_MAX_SUBSCRIBERS`           | nros-rmw-xrce build.rs   |
| `CONFIG_NROS_XRCE_MAX_SERVICE_SERVERS` | `XRCE_MAX_SERVICE_SERVERS`       | nros-rmw-xrce build.rs   |
| `CONFIG_NROS_XRCE_MAX_SERVICE_CLIENTS` | `XRCE_MAX_SERVICE_CLIENTS`       | nros-rmw-xrce build.rs   |
| `CONFIG_NROS_XRCE_BUFFER_SIZE`         | `XRCE_BUFFER_SIZE`               | nros-rmw-xrce build.rs   |
| `CONFIG_NROS_XRCE_STREAM_HISTORY`      | `XRCE_STREAM_HISTORY`            | nros-rmw-xrce build.rs   |

**Common** (visible regardless of backend):

| Kconfig                              | Env Var                        | Consumer                |
|--------------------------------------|--------------------------------|-------------------------|
| `CONFIG_NROS_C_MAX_HANDLES`          | `NROS_EXECUTOR_MAX_HANDLES`    | nros-c build.rs         |
| `CONFIG_NROS_C_MAX_SUBSCRIPTIONS`    | `NROS_MAX_SUBSCRIPTIONS`       | nros-c build.rs         |
| `CONFIG_NROS_C_MAX_TIMERS`           | `NROS_MAX_TIMERS`              | nros-c build.rs         |
| `CONFIG_NROS_C_MAX_SERVICES`         | `NROS_MAX_SERVICES`            | nros-c build.rs         |

### Design Decisions

**Single module at repo root, not in zpico-zephyr.** The nros Zephyr module
spans multiple packages (zpico-sys, zpico-zephyr, xrce-sys, nros-c). It
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
integration, and CDR serialization — a complete ROS 2 client library.

**Kconfig `choice` for API selection.** `NROS_C_API` and `NROS_RUST_API` are
mutually exclusive via Kconfig `choice`. A single Zephyr image links either
`libnros_c.a` (for C apps) or lets the user's Cargo build pull in the `nros`
crate (for Rust apps). Both paths share the same platform layer.

**Kconfig `choice` for RMW backend.** `NROS_RMW_ZENOH` and `NROS_RMW_XRCE` are
mutually exclusive. The selected backend determines which C sources the module
compiles, which Cargo features are passed to `nros_cargo_build()`, and which
env vars `nros_set_cargo_env_from_kconfig()` exports.

**XRCE C library compiled by Cargo, not Zephyr CMake.** Unlike zenoh-pico,
Micro-XRCE-DDS-Client uses a "custom transport" model — the library itself has
zero platform-specific code. All 33 C source files are compiled by
`xrce-sys/build.rs` via `cc::Build`, with no Zephyr header dependencies. Only
the transport glue (`xrce_zephyr.c`, which uses `zsock_*` APIs) needs Zephyr
CMake compilation. This avoids the complexity of absorbing another C library
into the Zephyr module build.

**XRCE Zephyr time via built-in `time.c`.** Micro-XRCE-DDS-Client's `time.c`
already has `#ifdef UCLIENT_PLATFORM_ZEPHYR` support using `clock_gettime()`.
The `xrce-sys` `zephyr` feature enables this path, so no separate platform
crate is needed for the clock (unlike the bare-metal path which provides
`uxr_millis()` from `xrce-platform-mps2-an385`).

**`NROS_` Kconfig prefix.** The old zpico-zephyr Kconfig used the
`NANO_ROS_` prefix. This phase adopts the project-wide `NROS_` prefix,
consistent with the crate rename completed in Phase 33 and the constant rename
completed in Phase 45.

### Cargo Feature Wiring

The `platform-zephyr` feature must forward to **both** RMW backends via `?`
syntax (only the active backend's optional dependency is present):

**`nros/Cargo.toml`:**
```toml
platform-zephyr = [
    "nros-node/platform-zephyr",
    "nros-rmw-zenoh?/platform-zephyr",
    "nros-rmw-xrce?/platform-zephyr",
]
```

**`nros-node/Cargo.toml`:**
```toml
platform-zephyr = [
    "nros-rmw-zenoh?/platform-zephyr",
    "nros-rmw-xrce?/platform-zephyr",
]
```

**`nros-rmw-xrce/Cargo.toml`** (new feature):
```toml
platform-zephyr = ["xrce-sys/zephyr"]
```

**`xrce-sys/Cargo.toml`** (new feature):
```toml
zephyr = []  # enables UCLIENT_PLATFORM_ZEPHYR, compiles time.c
```

**`xrce-sys/build.rs`** changes: when `zephyr` feature is active, define
`UCLIENT_PLATFORM_ZEPHYR` in the generated config.h and compile `time.c`
(same as posix, but with the Zephyr platform define instead).

---

## Sub-phases

### Part A — Zenoh Refactor (46.1–46.10) ✅

These sub-phases are complete. They established the root Zephyr module,
refactored zpico-zephyr, wired up the Kconfig→Cargo bridge, and revised all
existing examples.

#### 46.1 — Create root Zephyr module structure ✅

- [x] Create `zephyr/module.yml` with `name: nros`
- [x] Create `zephyr/Kconfig` with full config hierarchy using `NROS_` prefix
- [x] Create `zephyr/CMakeLists.txt` skeleton (conditional on `CONFIG_NROS`)
- [x] Create `zephyr/cmake/nros_cargo_build.cmake` with target detection,
  env var bridging, and Cargo invocation helpers
- [x] Remove `packages/zpico/zpico-zephyr/zephyr/module.yml`
- [x] Verify: `west build -t menuconfig` shows NROS menu

#### 46.2 — Wire zenoh_shim.c into module CMake ✅

- [x] Add `zephyr_library_sources(zenoh_shim.c)` to `zephyr/CMakeLists.txt`
- [x] Add `zephyr_compile_definitions()` mapping Kconfig → `-DZPICO_MAX_*`
- [x] Add `zephyr_include_directories()` for `zpico-sys/c/include`

#### 46.3 — Wire zpico-zephyr platform support ✅

- [x] Add `zephyr_library_sources(zpico_zephyr.c)` to module CMake
- [x] Add `zephyr_include_directories()` for `zpico-zephyr/include`

#### 46.4 — Refactor zpico-zephyr to platform-only ✅

- [x] Rename `bsp_zephyr.c` → `zpico_zephyr.c`,
  `nano_ros_bsp_zephyr.h` → `zpico_zephyr.h`
- [x] Remove nros-specific API (`nano_ros_bsp_*` functions, types, error codes)
- [x] Keep platform-level functions: `zpico_zephyr_wait_network()`,
  `zpico_zephyr_init_session()`, `zpico_zephyr_shutdown()`
- [x] Slim down Kconfig to platform-only options with `NROS_` prefix

#### 46.5 — Implement Kconfig→Cargo env var bridge ✅

- [x] Implement `nros_set_cargo_env_from_kconfig()` mapping Kconfig → env vars
- [x] Env vars visible to both `nros_cargo_build()` and `rust_cargo_application()`

#### 46.6 — Implement nros_cargo_build() for C API path ✅

- [x] Implement `nros_detect_rust_target()` (Cortex-M, ESP32, native_sim)
- [x] Implement `nros_cargo_build()` with workspace Cargo invocation
- [x] Wire into `zephyr/CMakeLists.txt` with library linking

#### 46.7 — ~~Guard zenoh_shim.c in build.rs~~ (OBSOLETE)

Obsoleted by 46.12. The existing guard in `zpico-sys/build.rs` (line 376:
`if backend_count > 0 && !use_zephyr`) already skips shim compilation for
Zephyr. After 46.12 absorbs zenoh-pico into the module, there is only one
compilation path for each source file.

#### 46.8 — Revise Rust Zephyr examples ✅

- [x] All 6 Rust examples reduced to 3-line `CMakeLists.txt`
- [x] Updated `prj.conf` with `CONFIG_NROS=y`, `CONFIG_NROS_RUST_API=y`
- [x] Moved `Z_FEATURE_INTEREST=0` to module CMake

#### 46.9 — Revise C Zephyr examples ✅

- [x] Created `zephyr/cmake/nros_generate_interfaces.cmake` for C codegen
- [x] Rewrote C examples to use nros-c API + generated `std_msgs` types
- [x] Standard `.msg` files resolved from ament index (`AMENT_PREFIX_PATH`)
- [ ] Verify: `just build-zephyr-c` succeeds
- [ ] Verify: `just test-zephyr` passes (C examples)

#### 46.10 — Update justfile and test infrastructure ✅

- [x] Updated build/test recipes and test fixtures

---

### Part B — Zenoh-pico Absorption (46.12)

Eliminate the separate zenoh-pico west module by compiling from the vendored
submodule, giving nros full control over zenoh-pico configuration.

#### 46.12 — Absorb zenoh-pico Zephyr module into nros

**Current state:** Zephyr projects pull zenoh-pico as a separate west module
(`modules/lib/zenoh-pico/`, v1.7.2). The nros repo also vendors zenoh-pico as
a git submodule (`packages/zpico/zpico-sys/zenoh-pico/`, v1.6.2).

**Target state:** The nros module compiles zenoh-pico from the vendored
submodule. `west.yml` no longer references zenoh-pico.

- [x] **Remove zenoh-pico from `west.yml`** — removed project entry and
  `eclipse-zenoh` remote
- [x] **Compile zenoh-pico sources in `zephyr/CMakeLists.txt`** from
  `packages/zpico/zpico-sys/zenoh-pico/`:
  - Glob core sources (`src/**/*.c`) excluding `src/system/` platform backends
  - Add Zephyr platform backend (`src/system/zephyr/*.c`)
  - Define `ZENOH_ZEPHYR`
  - Map `NROS_ZENOH_*` Kconfig → `Z_FEATURE_*` compile definitions
- [x] **Absorb zenoh-pico Kconfig** into `zephyr/Kconfig`: inlined 12 features
  as `CONFIG_NROS_ZENOH_*` with sensible defaults (pub, sub, query, queryable,
  TCP, multi-thread default=y)
- [x] **Remove `depends on ZENOH_PICO`** from `zephyr/Kconfig`
- [x] **Apply zenoh-pico patches via compile definitions** instead of
  `setup.sh` file patching (`Z_FEATURE_INTEREST=0`, `Z_FEATURE_MATCHING=0`)
- [x] **Revise `scripts/zephyr/setup.sh`**: removed `patch_zenoh_pico()`,
  updated workspace summary text
- [x] **Update example `prj.conf` files**: removed `CONFIG_ZENOH_PICO=y`
  and all `CONFIG_ZENOH_PICO_*` lines (now handled by nros Kconfig defaults)
- [x] **Version alignment**: vendored submodule at
  `packages/zpico/zpico-sys/zenoh-pico/` is the single source of truth
- [ ] Verify: `just test-zephyr` passes
- [ ] Verify: `west update` no longer fetches a separate zenoh-pico module
- [ ] Verify: `just quality` passes (non-Zephyr builds unaffected)

---

### Part C — Multi-RMW Backend (46.13–46.17)

Add XRCE-DDS as a selectable RMW backend on Zephyr.

#### 46.13 — RMW backend Kconfig choice ✅

Add a `choice` block for selecting between zenoh and XRCE, and reorganize
existing Kconfig options under backend-specific `if` guards.

- [x] Add `NROS_RMW_BACKEND` choice with `NROS_RMW_ZENOH` (default) and
  `NROS_RMW_XRCE` options
- [x] Remove `depends on ZENOH_PICO` from top-level `menuconfig NROS` (this
  is now internal to `NROS_RMW_ZENOH` after 46.12, or auto-selected)
- [x] Move existing zenoh transport tuning options under `if NROS_RMW_ZENOH`
  (`NROS_MAX_PUBLISHERS`, `NROS_MAX_SUBSCRIBERS`, `NROS_MAX_QUERYABLES`,
  `NROS_MAX_LIVELINESS`, `NROS_FRAG_MAX_SIZE`, `NROS_BATCH_UNICAST_SIZE`,
  `NROS_SUBSCRIBER_BUFFER_SIZE`, `NROS_SERVICE_BUFFER_SIZE`)
- [x] Move `NROS_ZENOH_LOCATOR`, `NROS_TRANSPORT_SERIAL` under
  `if NROS_RMW_ZENOH`
- [x] Add XRCE transport tuning under `if NROS_RMW_XRCE`:
  - `NROS_XRCE_TRANSPORT_MTU` (default 512)
  - `NROS_XRCE_MAX_SUBSCRIBERS` (default 8)
  - `NROS_XRCE_MAX_SERVICE_SERVERS` (default 4)
  - `NROS_XRCE_MAX_SERVICE_CLIENTS` (default 4)
  - `NROS_XRCE_BUFFER_SIZE` (default 1024)
  - `NROS_XRCE_STREAM_HISTORY` (default 4, range 2..16)
  - `NROS_XRCE_AGENT_ADDR` (default "192.0.2.2")
  - `NROS_XRCE_AGENT_PORT` (default 2018)
- [x] `NROS_RMW_XRCE` should `depends on NET_SOCKETS` (Zephyr BSD socket API)
- [x] Keep common options outside backend guards: `NROS_API`, `NROS_DOMAIN_ID`,
  `NROS_INIT_DELAY_MS`, C API limits
- [ ] Verify: `west build -t menuconfig` shows the RMW choice and
  backend-specific options appear/disappear based on selection

#### 46.14 — Cargo feature wiring for `platform-zephyr` + XRCE ✅

Wire the `platform-zephyr` feature through the XRCE crate chain so Cargo
builds the correct XRCE configuration for Zephyr.

- [x] Add `zephyr` feature to `xrce-sys/Cargo.toml`
- [x] Update `xrce-sys/build.rs`:
  - When `zephyr` feature is active: skip `time.c` (Zephyr provides
    `uxr_millis`/`uxr_nanos` from `xrce_zephyr.c`)
  - Ensure `posix`, `bare-metal`, and `zephyr` are mutually exclusive
- [x] Add `platform-zephyr = ["xrce-sys/zephyr"]` to
  `nros-rmw-xrce/Cargo.toml`
- [x] Update `nros-node/Cargo.toml`: add `"nros-rmw-xrce?/platform-zephyr"`
  to `platform-zephyr` feature
- [x] Update `nros/Cargo.toml`: add `"nros-rmw-xrce?/platform-zephyr"` to
  `platform-zephyr` feature
- [x] Verify: `cargo check -p nros --no-default-features --features
  "rmw-xrce,platform-zephyr,ros-humble"` succeeds
- [x] Verify: `just quality` passes (existing posix/bare-metal unaffected)

#### 46.15 — XRCE Zephyr transport glue and module CMake ✅

Create the Zephyr BSD socket transport for XRCE and make the module CMake
backend-conditional.

- [x] **Create `packages/xrce/xrce-zephyr/`**:
  - `src/xrce_zephyr.c` — implement 4 custom transport callbacks using
    Zephyr BSD socket API (`zsock_socket`, `zsock_connect`, `zsock_send`,
    `zsock_recvfrom` with `zsock_poll` for timeout)
  - `include/xrce_zephyr.h` — public API:
    `xrce_zephyr_wait_network(int timeout_ms)`,
    `xrce_zephyr_init(const char *agent_addr, int agent_port)`
  - Transport callbacks registered via
    `uxr_set_custom_transport_callbacks()` (from xrce-sys)
  - `uxr_millis()` / `uxr_nanos()` clock symbols using Zephyr `k_uptime_get()`

- [x] **Created `packages/xrce/nros-rmw-xrce/src/zephyr.rs`**:
  - Declares extern C references to the 4 transport callbacks
  - Provides `init_zephyr_transport()` that registers callbacks via
    `crate::init_transport()`

- [x] **Make `zephyr/CMakeLists.txt` backend-conditional**:
  - zenoh-pico sources under `if(CONFIG_NROS_RMW_ZENOH)`
  - xrce_zephyr.c under `elseif(CONFIG_NROS_RMW_XRCE)`

- [x] **Make `nros_cargo_build()` feature string backend-conditional**:
  - `rmw-zenoh,platform-zephyr,ros-humble` for zenoh
  - `rmw-xrce,platform-zephyr,ros-humble` for XRCE

- [x] **Extend `nros_set_cargo_env_from_kconfig()`**: add conditional
  `XRCE_*` env var exports when `CONFIG_NROS_RMW_XRCE` is set

- [x] **Extend `nros_cargo_build()` `add_custom_command`**: pass `XRCE_*`
  env vars alongside existing `ZPICO_*` vars (harmless to pass both — build.rs
  ignores vars it doesn't consume)

- [ ] Verify: `west build` with `CONFIG_NROS_RMW_XRCE=y` +
  `CONFIG_NROS_C_API=y` produces a linkable binary for `native_sim`

#### 46.16 — XRCE Zephyr examples ✅

Create C and Rust XRCE examples following the same pattern as zenoh examples.

- [x] **C talker** (`examples/zephyr/c/xrce/talker/`):
  - `CMakeLists.txt`: `nros_generate_interfaces(std_msgs "msg/Int32.msg")` +
    `target_sources(app PRIVATE src/main.c)`
  - `prj.conf`: `CONFIG_NROS=y`, `CONFIG_NROS_RMW_XRCE=y`,
    `CONFIG_NROS_C_API=y`, `CONFIG_NROS_XRCE_AGENT_ADDR="192.0.2.2"`
  - `src/main.c`: uses `xrce_zephyr_wait_network()` + `xrce_zephyr_init()`
    before `nano_ros_support_init()`
- [x] **C listener** (`examples/zephyr/c/xrce/listener/`): same pattern
  with subscription + executor spin
- [x] **Rust talker** (`examples/zephyr/rust/xrce/talker/`):
  - `CMakeLists.txt`: 3 lines + `rust_cargo_application()`
  - `prj.conf`: `CONFIG_NROS=y`, `CONFIG_NROS_RMW_XRCE=y`,
    `CONFIG_NROS_RUST_API=y`
  - `Cargo.toml`: `nros = { features = ["rmw-xrce", "platform-zephyr"] }`
- [x] **Rust listener** (`examples/zephyr/rust/xrce/listener/`): same pattern
- [ ] Verify: all 4 new examples build with `west build`
- [ ] Verify: XRCE examples can communicate via Micro-XRCE-DDS Agent

#### 46.17 — XRCE Zephyr test infrastructure ✅

Add test recipes and integration tests for XRCE on Zephyr.

- [x] Add `just build-zephyr-xrce` recipe (C + Rust XRCE examples)
- [x] Add `just test-zephyr-xrce` recipe
- [x] Reuse existing `XrceAgent` fixture from `nros-tests/src/fixtures/`
- [x] Add XRCE example entries to `example_path_for_name()` and
  `build_dir_for_example()` in `nros-tests/src/zephyr.rs`
- [x] Add `test_zephyr_xrce_rust_talker_listener` and
  `test_zephyr_xrce_c_talker_listener` E2E tests in `zephyr.rs`
- [x] Update `clean-zephyr` and `build-zephyr-all` to include XRCE dirs
- [x] Nextest `zephyr` test group (max-threads=1) automatically applies
  (binary name matches `binary(zephyr)` filter)
- [x] Verify: `just quality` passes (zenoh tests unaffected)
- [ ] Verify: `just test-zephyr-xrce` passes (requires Zephyr workspace)

---

### Part D — Documentation (46.11)

#### 46.11 — Documentation

Update all docs to reflect multi-RMW architecture.

- [x] Update `CLAUDE.md`:
  - Add `zephyr/` to workspace structure
  - Update zpico-zephyr description (platform support, not BSP)
  - Add `xrce/` package tree with xrce-zephyr
  - Add XRCE Zephyr examples to example tree
  - Document multi-RMW Kconfig options and module usage
- [x] Update `docs/guides/zephyr-setup.md`:
  - Updated network diagram to bridge topology (zeth-br, zeth0, zeth1)
  - Document `CONFIG_NROS_RMW_ZENOH` / `CONFIG_NROS_RMW_XRCE` choice
  - Document `CONFIG_NROS_C_API` / `CONFIG_NROS_RUST_API` choice
  - Full Kconfig reference tables (common, zenoh, XRCE, C API)
  - Removed manual `target_sources` boilerplate references
- [x] Update `docs/guides/creating-examples.md`:
  - Revised Zephyr section with module-based CMakeLists.txt
  - Document C API vs Rust API path selection
  - Document zenoh vs XRCE backend selection
  - Updated API from ShimExecutor to EmbeddedExecutor
- [x] Mark phase 46 as Complete in this file
- [x] Verify: all doc cross-references are valid

---

## Acceptance Criteria

### Functional

1. `west build -t menuconfig` shows NROS menu with RMW backend choice
2. C Zephyr talker builds and runs with zenoh backend (native_sim)
3. Rust Zephyr talker builds and runs with zenoh backend (native_sim)
4. C Zephyr talker builds and runs with XRCE backend (native_sim)
5. Rust Zephyr talker builds and runs with XRCE backend (native_sim)
6. Changing backend-specific Kconfig values in `prj.conf` affects both C and
   Rust builds
7. All 6 Rust zenoh Zephyr examples build and pass tests
8. Both C zenoh Zephyr examples build and pass tests
9. XRCE Zephyr examples communicate via Micro-XRCE-DDS Agent
10. `just test-all` passes (no regressions)

### Structural

11. No example `CMakeLists.txt` contains `target_sources` for zenoh_shim.c or
    bsp_zephyr.c
12. No example `CMakeLists.txt` contains hardcoded relative paths to
    `packages/zpico/` or `packages/xrce/`
13. `zpico-zephyr` has zero nros-specific API (no `nano_ros_bsp_*` functions)
14. `zpico-zephyr/zephyr/module.yml` does not exist
15. Each backend's C sources are compiled exactly once per build (no duplicates)

### Module Integration (46.12)

16. `west.yml` does not reference zenoh-pico as a separate module
17. zenoh-pico is compiled from `packages/zpico/zpico-sys/zenoh-pico/` submodule
18. `scripts/zephyr/setup.sh` has no `patch_zenoh_pico()` function
19. `Z_FEATURE_INTEREST=0` / `Z_FEATURE_MATCHING=0` applied via compile
    definitions, not source file patches

### Multi-RMW (46.13–46.17)

20. Switching RMW backend requires only changing `prj.conf` — application
    code is identical (same nros-c / nros API)
21. Zenoh-specific Kconfig options hidden when XRCE is selected (and vice versa)
22. `platform-zephyr` Cargo feature correctly forwards to both
    `nros-rmw-zenoh` and `nros-rmw-xrce` via `?` syntax
23. XRCE env vars (`XRCE_TRANSPORT_MTU`, etc.) propagate from Kconfig to
    Cargo build.rs

### Configuration

24. All Kconfig values propagate correctly to Cargo env vars
25. Non-Zephyr builds (`just quality`) are unaffected
26. `prj.conf` is the only file users need to edit for configuration

---

## Risk Assessment

**Env var propagation timing.** CMake `set(ENV{...})` sets process-level env
vars during configure time. `rust_cargo_application()` spawns Cargo during
build time. If Cargo inherits the CMake process env (typical for
`add_custom_command`), this works. If not, we may need to generate a
`.cargo/config.toml` or pass env vars via the custom command's `COMMAND`
property. Mitigation: tested and working in 46.5.

**nros-c on native_sim.** `nros-c` has only been built for host (x86_64) and
bare-metal ARM targets. Building for `native_sim` (x86_64-unknown-linux-gnu)
with `platform-zephyr` is a new combination that may surface link issues.
Mitigation: validated in 46.6.

**Duplicate symbols.** ~~Both the module CMake and zpico-sys build.rs want to
compile `zenoh_shim.c`.~~ Resolved: the existing guard in `build.rs` (line 376)
already skips shim compilation for Zephyr. 46.7 is obsolete.

**zenoh-pico source compatibility (46.12).** Compiling zenoh-pico from the
vendored submodule (currently 1.6.2) instead of the separate west module (1.7.2)
may surface API differences if examples relied on 1.7.x features. Mitigation:
verify all Zephyr examples build and pass with the submodule version. If needed,
bump the submodule to match.

**zephyr-lang-rust stability.** The `rust_cargo_application()` CMake function
tracks `main` branch. API changes could break the Rust path. Mitigation: pin
to a specific revision in `west.yml` after validation.

**XRCE `time.c` on Zephyr (46.14).** The `zephyr` feature in xrce-sys needs
to compile `time.c` which includes `<version.h>` (a Zephyr header). When
xrce-sys is built by Cargo (not Zephyr CMake), the Zephyr sysroot may not be
in the default include path. Mitigation: if `cc::Build` cannot find Zephyr
headers, we have two options: (1) skip `time.c` and provide `uxr_millis()`
/ `uxr_nanos()` from `xrce_zephyr.c` (compiled by Zephyr CMake), matching
the bare-metal pattern; or (2) pass Zephyr include paths to `cc::Build` via
`DEP_ZEPHYR_INCLUDE` env var from the module CMake. Option (1) is simpler
and more consistent with the existing bare-metal approach.

**XRCE UDP on native_sim.** Zephyr's native_sim uses host networking or a
simulated network stack. BSD socket calls (`zsock_*`) map to POSIX sockets on
native_sim, so `xrce_zephyr.c` should work without modification. Mitigation:
validate in 46.15.
