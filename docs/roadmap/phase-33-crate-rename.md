# Phase 33: Crate Rename (`nros-*` / `zpico-*`)

**Status: Not Started**

**Design doc:** `docs/design/rmw-layer-design.md`

## Goal

Rename all crates from `nano-ros-*` to shorter, semantically-grouped prefixes:
- **`nros-*`** — Core library (middleware-agnostic) + user-facing platform API
- **`nros-rmw-*`** — RMW glue (bridges middleware to nros traits)
- **`zpico-*`** — Zenoh-pico internals (no nros dependency)

This paves the way for the RMW abstraction layer (alternative middleware backends).

## Phases

### Phase A: Rename core crates (`nano-ros-*` → `nros-*`)

Rename the workspace-member core crates. These are the most referenced names.

| Step | Current           | New           | Notes               |
|------|-------------------|---------------|---------------------|
| A.1  | `nano-ros-core`   | `nros-core`   | Core types, traits  |
| A.2  | `nano-ros-serdes` | `nros-serdes` | CDR serialization   |
| A.3  | `nano-ros-macros` | `nros-macros` | Proc macros         |
| A.4  | `nano-ros-params` | `nros-params` | Parameter server    |
| A.5  | `nano-ros-node`   | `nros-node`   | High-level node API |
| A.6  | `nano-ros-c`      | `nros-c`      | C API               |
| A.7  | `nano-ros`        | `nros`        | Unified re-export   |

**Per-crate rename procedure:**
1. Rename directory (`packages/core/nano-ros-core/` → `packages/core/nros-core/`)
2. Update `Cargo.toml` (`name`, internal deps)
3. Update `lib.rs` crate-level attributes if any
4. Rename Rust module references (`nano_ros_core` → `nros_core`) in all dependents
5. Update root `Cargo.toml` workspace members list
6. Update `.cargo/config.toml` `[patch.crates-io]` in all examples
7. Update `package.xml` dependency names in examples (codegen uses these)
8. Run `just quality` after each crate

### Phase B: Rename transport/link crates

| Step | Current                                                     | New                           | Notes                                |
|------|-------------------------------------------------------------|-------------------------------|--------------------------------------|
| B.1  | `nano-ros-transport-zenoh-sys` (dir: `zenoh-pico-shim-sys`) | `zpico-sys`                   | FFI + zenoh-pico submodule           |
| B.2  | `nano-ros-transport-zenoh` (dir: `zenoh-pico-shim`)         | —                             | Absorbed into `nros-rmw-zenoh` (B.4) |
| B.3  | `nano-ros-link-smoltcp`                                     | `zpico-smoltcp`               | TCP via smoltcp                      |
| B.4  | `nano-ros-transport` + shim.rs                              | `nros-rmw` + `nros-rmw-zenoh` | Split: traits vs zenoh impl          |
| B.5  | `nano-ros-bsp-zephyr`                                       | `zpico-zephyr`                | Zephyr C integration                 |

**B.4 detail — transport split:**
- `nros-rmw` gets the trait definitions from `traits.rs` (middleware-agnostic)
- `nros-rmw-zenoh` gets `shim.rs` (zenoh trait impl) + content from current `nano-ros-transport-zenoh`
- Move keyexpr formatting out of `TopicInfo`/`ServiceInfo`/`ActionInfo` into `nros-rmw-zenoh`
- `nros-rmw` directory: `packages/core/nros-rmw/`
- `nros-rmw-zenoh` directory: `packages/zpico/nros-rmw-zenoh/`

### Phase C: Split and rename platform crates

Each current platform crate is a mix of zpico system symbols (55 `z_*`/`_z_*` FFI exports, no nros deps) and user-facing ROS API (Publisher\<M\>, Subscription\<M\>, run_node()). These must be split so the `nros-*` name is honest — middleware-agnostic user API only.

**C.1–C.4: Extract zpico system symbols into `zpico-platform-*` crates**

| Step | Source                         | New zpico crate          | Modules extracted                                                          |
|------|--------------------------------|--------------------------|----------------------------------------------------------------------------|
| C.1  | `nano-ros-platform-qemu`       | `zpico-platform-qemu`    | clock, memory, random, sleep, socket, threading, time, libc_stubs (727 lines) |
| C.2  | `nano-ros-platform-esp32`      | `zpico-platform-esp32`   | Same 8 modules                                                             |
| C.3  | `nano-ros-platform-esp32-qemu` | `zpico-platform-esp32-qemu` | Same 8 modules                                                          |
| C.4  | `nano-ros-platform-stm32f4`    | `zpico-platform-stm32f4` | Same 8 modules                                                             |

Each `zpico-platform-*` crate:
- Lives in `packages/zpico/zpico-platform-*/`
- Has NO nros dependencies (only cortex-m, esp-hal, etc.)
- Provides 55 `#[unsafe(no_mangle)]` FFI symbols required by zenoh-pico
- Is excluded from the default workspace (embedded-only, cross-compiled)

**C.5–C.8: Rename remaining platform code to `nros-*` board crates**

| Step | Source (after extraction)       | New nros crate    | Modules remaining                                              |
|------|--------------------------------|-------------------|----------------------------------------------------------------|
| C.5  | `nano-ros-platform-qemu`       | `nros-qemu`       | node, publisher, subscriber, config, error, timing + hw init   |
| C.6  | `nano-ros-platform-esp32`      | `nros-esp32`      | Same pattern                                                   |
| C.7  | `nano-ros-platform-esp32-qemu` | `nros-esp32-qemu` | Same pattern                                                   |
| C.8  | `nano-ros-platform-stm32f4`    | `nros-stm32f4`    | Same + phy, pins                                               |

Each `nros-*` board crate:
- Lives in `packages/boards/nros-*/`
- Depends on `nros-core`, `nros-rmw` (middleware-agnostic)
- Links `zpico-platform-*` + `zpico-smoltcp` for zenoh backend (via Cargo deps)
- Is excluded from the default workspace (embedded-only, cross-compiled)

**C.9: Update examples to depend on split crates**
- QEMU examples: depend on `nros-qemu` (which pulls in `zpico-platform-qemu` etc.)
- ESP32 examples: same pattern
- Update `.cargo/config.toml` patch entries

### Phase D: Rename testing + interfaces crates

| Step | Current               | New              | Notes                                        |
|------|-----------------------|------------------|----------------------------------------------|
| D.1  | `nano-ros-tests`      | `nros-tests`     | Integration test crate                       |
| D.2  | Update codegen output | —                | Generated code references `nros_core::` etc. |
| D.3  | `rcl-interfaces`      | `rcl-interfaces` | Keep name (it's a ROS 2 package name)        |

### Phase E: Directory restructuring

| Step | Description                                                           |
|------|-----------------------------------------------------------------------|
| E.1  | Create `packages/zpico/` directory                                    |
| E.2  | Move `zpico-sys` to `packages/zpico/zpico-sys/`                       |
| E.3  | Move `zpico-smoltcp` to `packages/zpico/zpico-smoltcp/`               |
| E.4  | Move `zpico-zephyr` to `packages/zpico/zpico-zephyr/`                 |
| E.5  | Move `nros-rmw-zenoh` to `packages/zpico/nros-rmw-zenoh/`             |
| E.6  | Move `zpico-platform-*` to `packages/zpico/zpico-platform-*/`         |
| E.7  | Move `nros-*` board crates to `packages/boards/nros-*/`               |
| E.8  | Remove empty `packages/transport/`, `packages/link/`, `packages/bsp/`, `packages/platform/` |
| E.9  | Update all path references (Cargo.toml, .cargo/config.toml, scripts)  |

### Phase F: Update docs, CI, and scripts

| Step | Description                                                              |
|------|--------------------------------------------------------------------------|
| F.1  | Update CLAUDE.md workspace structure tree                                |
| F.2  | Update active docs with new crate names                                  |
| F.3  | Update justfile recipe names if needed                                   |
| F.4  | Update test infrastructure (fixture paths, binary names)                 |
| F.5  | Update CMake integration (`FindNanoRos.cmake` → adapt for new lib names) |
| F.6  | Delete `c/platform_smoltcp/` (superseded by `zpico-smoltcp`)             |
| F.7  | Final `just quality` verification                                        |

## Future Work (not in Phase 33)

These are enabled by the rename but implemented separately:

- **RMW trait abstraction**: Add `Rmw`, `Session` factory traits in `nros-rmw`. See `docs/design/rmw-layer-design.md` "RMW Trait Changes".
- **Alternative middleware**: MQTT-SN, native Zenoh backends.
- **Crates.io publishing**: Publish `nros-*` crates.

## Target Directory Layout

```
packages/
  core/                              # Core nros packages (middleware-agnostic)
    nros/                            #   Unified re-export
    nros-core/                       #   Core types, traits, lifecycle
    nros-serdes/                     #   CDR serialization
    nros-macros/                     #   Proc macros
    nros-params/                     #   Parameter server
    nros-rmw/                        #   RMW abstraction traits
    nros-node/                       #   High-level node API (desktop)
    nros-c/                          #   C API
  zpico/                             # Zenoh-pico internals (NO nros deps)
    zpico-sys/                       #   FFI + C shim + zenoh-pico submodule
    zpico-smoltcp/                   #   TCP via smoltcp for zenoh-pico
    zpico-platform-qemu/             #   System symbols for QEMU (z_malloc, etc.)
    zpico-platform-esp32/            #   System symbols for ESP32 WiFi
    zpico-platform-esp32-qemu/       #   System symbols for ESP32 QEMU
    zpico-platform-stm32f4/          #   System symbols for STM32F4
    zpico-zephyr/                    #   Zephyr C convenience library
    nros-rmw-zenoh/                  #   RMW glue (bridges zpico ↔ nros-rmw)
  boards/                            # User-facing platform packages (nros deps)
    nros-qemu/                       #   QEMU: Publisher<M>, run_node(), Config
    nros-esp32/                      #   ESP32-C3 WiFi user API
    nros-esp32-qemu/                 #   ESP32-C3 QEMU user API
    nros-stm32f4/                    #   STM32F4 user API
  drivers/                           # Hardware drivers (unchanged)
    lan9118-smoltcp/
    openeth-smoltcp/
  interfaces/                        # Generated ROS 2 types
    rcl-interfaces/
  testing/                           # Test infrastructure
    nros-tests/
  codegen/                           # Message binding generator
```

## Ordering Notes

- **Phase A first**: Core crates are the most-referenced, and all other phases depend on updated core names.
- **B.4 is hard**: Splitting `nano-ros-transport` into `nros-rmw` + `nros-rmw-zenoh` requires moving code between crates and updating the trait boundary.
- **C.1–C.4 before C.5–C.8**: Extract zpico symbols first, then rename the remaining user API.
- **Phase E can interleave**: Directory moves can happen alongside renames within each phase.
- **Each step should pass `just quality`** before proceeding to the next.
