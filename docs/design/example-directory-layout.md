# Example Directory Layout

**Status:** Proposed

## Overview

Reorganize the `examples/` directory from a flat `platform/example-name` layout to a structured `platform/language/rmw/use-case` hierarchy. This accommodates multiple RMW backends (zenoh-pico, XRCE-DDS), multiple languages (Rust, C), and cleanly separates transport-dependent examples from standalone validation code.

## Motivation

The current layout has several issues:

1. **No RMW dimension** — with XRCE-DDS arriving (phase 34), examples need to distinguish zenoh vs xrce variants
2. **Inconsistent naming** — `rs-talker`, `bsp-talker`, `c-talker` use ad-hoc prefixes to encode language/API level
3. **Duplicate examples** — `qemu/rs-talker` and `qemu/bsp-talker` are identical (same deps, same source)
4. **Platform mixing** — `esp32/qemu-talker` puts QEMU ESP32 examples under the real ESP32 directory
5. **Scattered locations** — 7 examples live in `packages/reference/` instead of `examples/`
6. **Dead code** — `embedded-cpp-talker` and `embedded-cpp-listener` depend on non-existent `nros-cpp`

## Directory Structure

Layout follows: `examples/{platform}/{language}/{rmw}/{use-case}/`

```
examples/
├── native/
│   ├── rust/
│   │   └── zenoh/
│   │       ├── talker/
│   │       ├── listener/
│   │       ├── service-server/
│   │       ├── service-client/
│   │       ├── action-server/
│   │       ├── action-client/
│   │       └── custom-msg/
│   └── c/
│       └── zenoh/
│           ├── talker/
│           ├── listener/
│           ├── custom-msg/
│           └── baremetal-demo/
│
├── qemu-arm/
│   └── rust/
│       ├── zenoh/
│       │   ├── talker/
│       │   └── listener/
│       ├── xrce/
│       │   └── (phase 34 examples)
│       ├── core/
│       │   ├── cdr-test/
│       │   └── wcet-bench/
│       └── standalone/
│           └── lan9118/
│
├── qemu-esp32/
│   └── rust/
│       └── zenoh/
│           ├── talker/
│           └── listener/
│
├── esp32/
│   └── rust/
│       ├── zenoh/
│       │   ├── talker/
│       │   └── listener/
│       └── standalone/
│           └── hello-world/
│
├── stm32f4/
│   └── rust/
│       ├── zenoh/
│       │   ├── talker/
│       │   ├── polling/
│       │   └── rtic/
│       ├── core/
│       │   └── embassy/
│       └── standalone/
│           └── smoltcp/
│
└── zephyr/
    ├── rust/
    │   └── zenoh/
    │       ├── talker/
    │       ├── listener/
    │       ├── service-server/
    │       ├── service-client/
    │       ├── action-server/
    │       └── action-client/
    └── c/
        └── zenoh/
            ├── talker/
            └── listener/
```

## Dimensions

### Platform (level 1)

The hardware target and toolchain. Determines `.cargo/config.toml`, linker scripts, and build target.

| Platform | Target | Machine |
|----------|--------|---------|
| `native` | `x86_64-unknown-linux-gnu` | Desktop Linux |
| `qemu-arm` | `thumbv7m-none-eabi` | QEMU MPS2-AN385 / lm3s6965evb |
| `qemu-esp32` | `riscv32imc-unknown-none-elf` | QEMU ESP32-C3 |
| `esp32` | `riscv32imc-unknown-none-elf` | Real ESP32-C3 hardware |
| `stm32f4` | `thumbv7em-none-eabihf` | Real STM32F4 (Nucleo-F429ZI) |
| `zephyr` | `native_sim/native/64` | Zephyr RTOS (built via west) |

### Language (level 2)

The programming language. Only present as a directory level when the platform has multiple languages.

| Language | Description |
|----------|-------------|
| `rust` | Rust examples (majority) |
| `c` | C examples using nros-c API + CMake |

### RMW (level 3)

The middleware transport backend. This is the key new dimension.

| RMW tier | Description | nros dependency | Transport dependency |
|----------|-------------|-----------------|----------------------|
| `zenoh` | Uses zenoh-pico transport | Yes | nros-rmw-zenoh or board crate |
| `xrce` | Uses XRCE-DDS transport | Yes | nros-rmw-xrce or board crate |
| `core` | Uses nros core only (CDR, Node API, safety benchmarks) | Yes | None |
| `standalone` | No nros dependency (driver validation, platform bring-up) | No | None |

### Use case (level 4)

The leaf directory. Describes what the example demonstrates.

| Use case | Description |
|----------|-------------|
| `talker` | Publisher (Int32 on `/chatter`) |
| `listener` | Subscriber |
| `service-server` | ROS 2 service server |
| `service-client` | ROS 2 service client |
| `action-server` | ROS 2 action server |
| `action-client` | ROS 2 action client |
| `custom-msg` | Custom message type generation |
| `cdr-test` | CDR serialization validation |
| `wcet-bench` | WCET cycle-count benchmarks |
| `lan9118` | LAN9118 Ethernet driver test |
| `smoltcp` | smoltcp TCP/IP stack validation |
| `embassy` | Embassy async framework integration |
| `rtic` | RTIC real-time framework integration |
| `polling` | Bare-metal polling loop |
| `hello-world` | Minimal platform bring-up |
| `baremetal-demo` | C bare-metal API demonstration |

## Migration Map

### Moves

| Current path | New path |
|---|---|
| `examples/native/rs-talker` | `examples/native/rust/zenoh/talker` |
| `examples/native/rs-listener` | `examples/native/rust/zenoh/listener` |
| `examples/native/rs-service-server` | `examples/native/rust/zenoh/service-server` |
| `examples/native/rs-service-client` | `examples/native/rust/zenoh/service-client` |
| `examples/native/rs-action-server` | `examples/native/rust/zenoh/action-server` |
| `examples/native/rs-action-client` | `examples/native/rust/zenoh/action-client` |
| `examples/native/rs-custom-msg` | `examples/native/rust/zenoh/custom-msg` |
| `examples/native/c-talker` | `examples/native/c/zenoh/talker` |
| `examples/native/c-listener` | `examples/native/c/zenoh/listener` |
| `examples/native/c-custom-msg` | `examples/native/c/zenoh/custom-msg` |
| `examples/native/c-baremetal-demo` | `examples/native/c/zenoh/baremetal-demo` |
| `examples/qemu/bsp-talker` | `examples/qemu-arm/rust/zenoh/talker` |
| `examples/qemu/bsp-listener` | `examples/qemu-arm/rust/zenoh/listener` |
| `examples/qemu/rs-test` | `examples/qemu-arm/rust/core/cdr-test` |
| `examples/qemu/rs-wcet-bench` | `examples/qemu-arm/rust/core/wcet-bench` |
| `examples/esp32/bsp-talker` | `examples/esp32/rust/zenoh/talker` |
| `examples/esp32/bsp-listener` | `examples/esp32/rust/zenoh/listener` |
| `examples/esp32/hello-world` | `examples/esp32/rust/standalone/hello-world` |
| `examples/esp32/qemu-talker` | `examples/qemu-esp32/rust/zenoh/talker` |
| `examples/esp32/qemu-listener` | `examples/qemu-esp32/rust/zenoh/listener` |
| `examples/stm32f4/bsp-talker` | `examples/stm32f4/rust/zenoh/talker` |
| `examples/zephyr/rs-talker` | `examples/zephyr/rust/zenoh/talker` |
| `examples/zephyr/rs-listener` | `examples/zephyr/rust/zenoh/listener` |
| `examples/zephyr/rs-service-server` | `examples/zephyr/rust/zenoh/service-server` |
| `examples/zephyr/rs-service-client` | `examples/zephyr/rust/zenoh/service-client` |
| `examples/zephyr/rs-action-server` | `examples/zephyr/rust/zenoh/action-server` |
| `examples/zephyr/rs-action-client` | `examples/zephyr/rust/zenoh/action-client` |
| `examples/zephyr/c-talker` | `examples/zephyr/c/zenoh/talker` |
| `examples/zephyr/c-listener` | `examples/zephyr/c/zenoh/listener` |
| `packages/reference/qemu-lan9118` | `examples/qemu-arm/rust/standalone/lan9118` |
| `packages/reference/stm32f4-polling` | `examples/stm32f4/rust/zenoh/polling` |
| `packages/reference/stm32f4-rtic` | `examples/stm32f4/rust/zenoh/rtic` |
| `packages/reference/stm32f4-embassy` | `examples/stm32f4/rust/core/embassy` |
| `packages/reference/stm32f4-smoltcp` | `examples/stm32f4/rust/standalone/smoltcp` |

### Deletions

| Path | Reason |
|---|---|
| `examples/qemu/rs-talker` | Duplicate of `bsp-talker` (identical deps and source) |
| `examples/qemu/rs-listener` | Duplicate of `bsp-listener` (identical deps and source) |
| `packages/reference/embedded-cpp-talker` | Depends on non-existent `nros-cpp` crate |
| `packages/reference/embedded-cpp-listener` | Depends on non-existent `nros-cpp` crate |

### Unchanged

| Path | Reason |
|---|---|
| `packages/reference/qemu-smoltcp-bridge` | Library (`src/lib.rs`), not an example |

## Glob Patterns

After the reorganization, examples are at depth 4 (`platform/language/rmw/use-case`):

```bash
# Find all Rust example crates
find examples -name Cargo.toml -mindepth 4

# Platform-specific discovery
find examples/qemu-arm -name Cargo.toml -mindepth 3
find examples/native -name Cargo.toml -mindepth 3
```

The `justfile` auto-discovery recipes should use `find` with `-mindepth 4` rather than hardcoded lists. See `docs/roadmap/phase-33-crate-rename.md` section "Justfile Recipe Revision" for the full plan.

## Per-Example Update Checklist

For each moved example:

1. `git mv` the directory to its new path
2. Update `Cargo.toml` path dependencies (e.g., `path = "../../../packages/core/nros"` → adjust depth)
3. Update `.cargo/config.toml` `[patch.crates-io]` paths
4. Update `CMakeLists.txt` paths (for C examples)
5. Update `justfile` recipes referencing the old path
6. Update `CLAUDE.md` workspace structure tree
7. Update integration test fixtures referencing example binary paths
8. Update `docs/` references to example paths
9. Run `just quality` after each platform group
