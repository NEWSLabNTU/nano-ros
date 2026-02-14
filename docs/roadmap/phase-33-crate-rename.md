# Phase 33: Crate Rename (`nros-*` / `zpico-*`)

**Status: 33.1 Complete, 33.2 Complete, 33.3 Not Started**

**Design doc:** `docs/design/rmw-layer-design.md`

## Goal

Rename all crates from `nano-ros-*` to shorter, semantically-grouped prefixes:
- **`nros-*`** — Core library (middleware-agnostic) + user-facing platform API
- **`nros-rmw-*`** — RMW glue (bridges middleware to nros traits)
- **`zpico-*`** — Zenoh-pico internals (no nros dependency)

This paves the way for the RMW abstraction layer (alternative middleware backends).

## Steps

### 33.1: Rename core crates (`nano-ros-*` → `nros-*`)

Rename the workspace-member core crates. These are the most referenced names.

| Current           | New           | Notes               |
|-------------------|---------------|---------------------|
| `nros-core`   | `nros-core`   | Core types, traits  |
| `nros-serdes` | `nros-serdes` | CDR serialization   |
| `nros-macros` | `nros-macros` | Proc macros         |
| `nros-params` | `nros-params` | Parameter server    |
| `nros-node`   | `nros-node`   | High-level node API |
| `nros-c`      | `nros-c`      | C API               |
| `nros`        | `nros`        | Unified re-export   |

**Per-crate rename procedure:**
1. Rename directory (`packages/core/nros-core/` → `packages/core/nros-core/`)
2. Update `Cargo.toml` (`name`, internal deps)
3. Update `lib.rs` crate-level attributes if any
4. Rename Rust module references (`nros_core` → `nros_core`) in all dependents
5. Update root `Cargo.toml` workspace members list
6. Update `.cargo/config.toml` `[patch.crates-io]` in all examples
7. Update `package.xml` dependency names in examples (codegen uses these)
8. Run `just quality` after each crate

### 33.2: Rename transport/link crates

| Current                                                     | New                           | Notes                                |
|-------------------------------------------------------------|-------------------------------|--------------------------------------|
| `nano-ros-transport-zenoh-sys` (dir: `zenoh-pico-shim-sys`) | `zpico-sys`                   | FFI + zenoh-pico submodule           |
| `nano-ros-transport-zenoh` (dir: `zenoh-pico-shim`)         | —                             | Absorbed into `nros-rmw-zenoh`       |
| `nano-ros-link-smoltcp`                                     | `zpico-smoltcp`               | TCP via smoltcp                      |
| `nano-ros-transport` + shim.rs                              | `nros-rmw` + `nros-rmw-zenoh` | Split: traits vs zenoh impl          |
| `nano-ros-bsp-zephyr`                                       | `zpico-zephyr`                | Zephyr C integration                 |

**Transport split detail:**
- `nros-rmw` gets the trait definitions from `traits.rs` (middleware-agnostic)
- `nros-rmw-zenoh` gets `shim.rs` (zenoh trait impl) + content from current `nano-ros-transport-zenoh`
- Move keyexpr formatting out of `TopicInfo`/`ServiceInfo`/`ActionInfo` into `nros-rmw-zenoh`
- `nros-rmw` directory: `packages/core/nros-rmw/`
- `nros-rmw-zenoh` directory: `packages/zpico/nros-rmw-zenoh/`

### 33.3: Split and rename platform crates

Each current platform crate is a mix of zpico system symbols (55 `z_*`/`_z_*` FFI exports, no nros deps) and user-facing ROS API (Publisher\<M\>, Subscription\<M\>, run_node()). These must be split so the `nros-*` name is honest — middleware-agnostic user API only.

**Extract zpico system symbols into `zpico-platform-*` crates:**

| Source                         | New zpico crate             | Modules extracted                                                             |
|--------------------------------|-----------------------------|-------------------------------------------------------------------------------|
| `nano-ros-platform-qemu`       | `zpico-platform-qemu`       | clock, memory, random, sleep, socket, threading, time, libc_stubs (727 lines) |
| `nano-ros-platform-esp32`      | `zpico-platform-esp32`      | Same 8 modules                                                                |
| `nano-ros-platform-esp32-qemu` | `zpico-platform-esp32-qemu` | Same 8 modules                                                                |
| `nano-ros-platform-stm32f4`    | `zpico-platform-stm32f4`    | Same 8 modules                                                                |

Each `zpico-platform-*` crate:
- Lives in `packages/zpico/zpico-platform-*/`
- Has NO nros dependencies (only cortex-m, esp-hal, etc.)
- Provides 55 `#[unsafe(no_mangle)]` FFI symbols required by zenoh-pico
- Is excluded from the default workspace (embedded-only, cross-compiled)

**Rename remaining platform code to `nros-*` board crates:**

| Source (after extraction)      | New nros crate    | Modules remaining                                            |
|--------------------------------|-------------------|--------------------------------------------------------------|
| `nano-ros-platform-qemu`       | `nros-qemu`       | node, publisher, subscriber, config, error, timing + hw init |
| `nano-ros-platform-esp32`      | `nros-esp32`      | Same pattern                                                 |
| `nano-ros-platform-esp32-qemu` | `nros-esp32-qemu` | Same pattern                                                 |
| `nano-ros-platform-stm32f4`    | `nros-stm32f4`    | Same + phy, pins                                             |

Each `nros-*` board crate:
- Lives in `packages/boards/nros-*/`
- Depends on `nros-core`, `nros-rmw` (middleware-agnostic)
- Links `zpico-platform-*` + `zpico-smoltcp` for zenoh backend (via Cargo deps)
- Is excluded from the default workspace (embedded-only, cross-compiled)

**Update examples to depend on split crates:**
- QEMU examples: depend on `nros-qemu` (which pulls in `zpico-platform-qemu` etc.)
- ESP32 examples: same pattern
- Update `.cargo/config.toml` patch entries

### 33.4: Rename testing, verification, and interfaces crates

| Current                  | New                  | Notes                                                 |
|--------------------------|----------------------|-------------------------------------------------------|
| `nano-ros-tests`         | `nros-tests`         | Integration test crate                                |
| `nano-ros-ghost-types`   | `nros-ghost-types`   | Ghost model types (workspace member)                  |
| `nano-ros-verification`  | `nros-verification`  | Verus proofs (excluded from workspace)                |
| Update codegen output    | —                    | Generated code references `nros_core::` etc.          |
| `rcl-interfaces`         | `rcl-interfaces`     | Keep name (it's a ROS 2 package name)                 |

**Verification detail:** `nros-verification` depends on core crates via path. Its `Cargo.toml` deps (`nros-serdes`, `nros-core`, `nros-params`, `nros-node`, `nano-ros-ghost-types`) must all update to new names. Verus `assume_specification` and `external_type_specification` references use Rust module paths (`nros_core::`, `nros_node::`, etc.) — all must be updated in proof modules.

### 33.5: Directory restructuring

| Description                                                                                 |
|---------------------------------------------------------------------------------------------|
| Create `packages/zpico/` directory                                                          |
| Move `zpico-sys` to `packages/zpico/zpico-sys/`                                             |
| Move `zpico-smoltcp` to `packages/zpico/zpico-smoltcp/`                                     |
| Move `zpico-zephyr` to `packages/zpico/zpico-zephyr/`                                       |
| Move `nros-rmw-zenoh` to `packages/zpico/nros-rmw-zenoh/`                                   |
| Move `zpico-platform-*` to `packages/zpico/zpico-platform-*/`                               |
| Move `nros-*` board crates to `packages/boards/nros-*/`                                     |
| Remove empty `packages/transport/`, `packages/link/`, `packages/bsp/`, `packages/platform/` |
| Update all path references (Cargo.toml, .cargo/config.toml, scripts)                        |

### 33.6: Update docs, CI, and scripts

| Description                                                              |
|--------------------------------------------------------------------------|
| Update CLAUDE.md workspace structure tree                                |
| Update active docs with new crate names                                  |
| Update justfile recipe names if needed                                   |
| Update test infrastructure (fixture paths, binary names)                 |
| Update CMake integration (`FindNanoRos.cmake` → adapt for new lib names) |
| Delete `c/platform_smoltcp/` (superseded by `zpico-smoltcp`)             |
| Final `just quality` verification                                        |

## Future Work (not in Phase 33)

These are enabled by the rename but implemented separately:

- **Phase 34: RMW abstraction + XRCE-DDS**: Formalize `Rmw`/`Session` factory traits in `nros-rmw`, refactor board crates to use abstract traits (remove `zenoh_shim_*` FFI calls), implement XRCE-DDS as second backend. See `docs/roadmap/phase-34-rmw-abstraction.md`.
- **Alternative middleware**: MQTT-SN, native Zenoh backends. See `docs/design/rmw-layer-design.md` "Complexity Assessment".
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
  verification/                      # Formal verification
    nros-ghost-types/                #   Ghost model types (workspace member)
    nros-verification/               #   Verus proofs (excluded from workspace)
  codegen/                           # Message binding generator
```

## Ordering Notes

- **33.1 first**: Core crates are the most-referenced, and all other steps depend on updated core names.
- **33.2 transport split is hard**: Splitting `nano-ros-transport` into `nros-rmw` + `nros-rmw-zenoh` requires moving code between crates and updating the trait boundary.
- **33.3 extract before rename**: Extract zpico symbols first, then rename the remaining user API.
- **33.5 can interleave**: Directory moves can happen alongside renames within each step.
- **Each step should pass `just quality`** before proceeding to the next.
