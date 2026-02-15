# Phase 33: Crate Rename (`nros-*` / `zpico-*`)

**Status: Complete (33.1–33.7)**

**Design docs:**
- `docs/design/rmw-layer-design.md` — crate rename plan
- `docs/design/example-directory-layout.md` — example directory reorganization

## Goal

Rename all crates from `nano-ros-*` to shorter, semantically-grouped prefixes:
- **`nros-*`** — Core library (middleware-agnostic) + user-facing platform API
- **`nros-rmw-*`** — RMW glue (bridges middleware to nros traits)
- **`zpico-*`** — Zenoh-pico internals (no nros dependency)

This paves the way for the RMW abstraction layer (alternative middleware backends).

## Steps

### 33.1: Rename core crates (`nano-ros-*` → `nros-*`)

Rename the workspace-member core crates. These are the most referenced names.

| Current       | New           | Notes               |
|---------------|---------------|---------------------|
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

### 33.3: Split and rename platform crates (Complete)

Each former platform crate was a mix of zpico system symbols (55 `z_*`/`_z_*` FFI exports, no nros deps) and user-facing ROS API (Publisher\<M\>, Subscription\<M\>, run_node()). These were split so the `nros-*` name is honest — middleware-agnostic user API only.

**Extracted zpico system symbols into `zpico-platform-*` crates:**

| Source                         | New zpico crate             | Modules extracted                                                             |
|--------------------------------|-----------------------------|-------------------------------------------------------------------------------|
| `nano-ros-platform-qemu`       | `zpico-platform-mps2-an385`       | clock, memory, random, sleep, socket, threading, time, libc_stubs (727 lines) |
| `nano-ros-platform-esp32`      | `zpico-platform-esp32`      | Same 8 modules                                                                |
| `nano-ros-platform-esp32-qemu` | `zpico-platform-esp32-qemu` | Same 8 modules                                                                |
| `nano-ros-platform-stm32f4`    | `zpico-platform-stm32f4`    | Same 8 modules                                                                |

Each `zpico-platform-*` crate:
- Lives in `packages/zpico/zpico-platform-*/`
- Has NO nros dependencies (only cortex-m, esp-hal, etc.)
- Provides 55 `#[unsafe(no_mangle)]` FFI symbols required by zenoh-pico
- Is excluded from the default workspace (embedded-only, cross-compiled)

**Renamed remaining platform code to `nros-*` board crates:**

| Source (after extraction)      | New nros crate    | Modules remaining                                            |
|--------------------------------|-------------------|--------------------------------------------------------------|
| `nano-ros-platform-qemu`       | `nros-mps2-an385`       | node, publisher, subscriber, config, error, timing + hw init |
| `nano-ros-platform-esp32`      | `nros-esp32`      | Same pattern                                                 |
| `nano-ros-platform-esp32-qemu` | `nros-esp32-qemu` | Same pattern                                                 |
| `nano-ros-platform-stm32f4`    | `nros-stm32f4`    | Same + phy, pins                                             |

Each `nros-*` board crate:
- Lives in `packages/boards/nros-*/`
- Depends on `nros-core`, `nros-rmw` (middleware-agnostic)
- Links `zpico-platform-*` + `zpico-smoltcp` for zenoh backend (via Cargo deps)
- Is excluded from the default workspace (embedded-only, cross-compiled)

**Updated examples to depend on split crates:**
- QEMU examples: depend on `nros-mps2-an385` (which pulls in `zpico-platform-mps2-an385` etc.)
- ESP32 examples: same pattern
- Updated `.cargo/config.toml` patch entries

### 33.4: Rename testing, verification, and interfaces crates

| Current                  | New                  | Notes                                                 |
|--------------------------|----------------------|-------------------------------------------------------|
| `nano-ros-tests`         | `nros-tests`         | Integration test crate                                |
| `nano-ros-ghost-types`   | `nros-ghost-types`   | Ghost model types (workspace member)                  |
| `nano-ros-verification`  | `nros-verification`  | Verus proofs (excluded from workspace)                |
| Update codegen output    | —                    | Generated code references `nros_core::` etc.          |
| `rcl-interfaces`         | `rcl-interfaces`     | Keep name (it's a ROS 2 package name)                 |

**Verification detail:** `nros-verification` depends on core crates via path. Its `Cargo.toml` deps (`nros-serdes`, `nros-core`, `nros-params`, `nros-node`, `nros-ghost-types`) must all update to new names. Verus `assume_specification` and `external_type_specification` references use Rust module paths (`nros_core::`, `nros_node::`, etc.) — all must be updated in proof modules.

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
| Rename `c/platform_smoltcp/` → `c/platform/` (generic bare-metal headers)|
| Delete legacy `c/platform/system.c` and `c/platform/network.c`          |
| Remove dead `use_c_network_shim`/`use_c_system_shim` code from build.rs |
| Final `just quality` verification                                        |

### 33.7: Reorganize examples directory

Restructure `examples/` from flat `platform/example-name` to `platform/language/rmw/use-case` hierarchy. Move example binaries out of `packages/reference/` into `examples/`. Delete dead C++ examples.

**Design doc:** `docs/design/example-directory-layout.md`

**Moves (30 examples):**

| Current                             | New                                          |
|-------------------------------------|----------------------------------------------|
| `examples/native/rs-talker`         | `examples/native/rust/zenoh/talker`          |
| `examples/native/rs-listener`       | `examples/native/rust/zenoh/listener`        |
| `examples/native/rs-service-server` | `examples/native/rust/zenoh/service-server`  |
| `examples/native/rs-service-client` | `examples/native/rust/zenoh/service-client`  |
| `examples/native/rs-action-server`  | `examples/native/rust/zenoh/action-server`   |
| `examples/native/rs-action-client`  | `examples/native/rust/zenoh/action-client`   |
| `examples/native/rs-custom-msg`     | `examples/native/rust/zenoh/custom-msg`      |
| `examples/native/c-talker`          | `examples/native/c/zenoh/talker`             |
| `examples/native/c-listener`        | `examples/native/c/zenoh/listener`           |
| `examples/native/c-custom-msg`      | `examples/native/c/zenoh/custom-msg`         |
| `examples/native/c-baremetal-demo`  | `examples/native/c/zenoh/baremetal-demo`     |
| `examples/qemu/bsp-talker`          | `examples/qemu-arm/rust/zenoh/talker`        |
| `examples/qemu/bsp-listener`        | `examples/qemu-arm/rust/zenoh/listener`      |
| `examples/qemu/rs-test`             | `examples/qemu-arm/rust/core/cdr-test`       |
| `examples/qemu/rs-wcet-bench`       | `examples/qemu-arm/rust/core/wcet-bench`     |
| `examples/esp32/bsp-talker`         | `examples/esp32/rust/zenoh/talker`           |
| `examples/esp32/bsp-listener`       | `examples/esp32/rust/zenoh/listener`         |
| `examples/esp32/hello-world`        | `examples/esp32/rust/standalone/hello-world` |
| `examples/esp32/qemu-talker`        | `examples/qemu-esp32/rust/zenoh/talker`      |
| `examples/esp32/qemu-listener`      | `examples/qemu-esp32/rust/zenoh/listener`    |
| `examples/stm32f4/bsp-talker`       | `examples/stm32f4/rust/zenoh/talker`         |
| `examples/zephyr/rs-talker`         | `examples/zephyr/rust/zenoh/talker`          |
| `examples/zephyr/rs-listener`       | `examples/zephyr/rust/zenoh/listener`        |
| `examples/zephyr/rs-service-server` | `examples/zephyr/rust/zenoh/service-server`  |
| `examples/zephyr/rs-service-client` | `examples/zephyr/rust/zenoh/service-client`  |
| `examples/zephyr/rs-action-server`  | `examples/zephyr/rust/zenoh/action-server`   |
| `examples/zephyr/rs-action-client`  | `examples/zephyr/rust/zenoh/action-client`   |
| `examples/zephyr/c-talker`          | `examples/zephyr/c/zenoh/talker`             |
| `examples/zephyr/c-listener`        | `examples/zephyr/c/zenoh/listener`           |

**Moves from `packages/reference/` (5 examples):**

| Current                              | New                                         |
|--------------------------------------|---------------------------------------------|
| `packages/reference/qemu-lan9118`    | `examples/qemu-arm/rust/standalone/lan9118` |
| `packages/reference/stm32f4-polling` | `examples/stm32f4/rust/zenoh/polling`       |
| `packages/reference/stm32f4-rtic`    | `examples/stm32f4/rust/zenoh/rtic`          |
| `packages/reference/stm32f4-embassy` | `examples/stm32f4/rust/core/embassy`        |
| `packages/reference/stm32f4-smoltcp` | `examples/stm32f4/rust/standalone/smoltcp`  |

**Deletions (4 items):**

| Path                                       | Reason                                                  |
|--------------------------------------------|---------------------------------------------------------|
| `examples/qemu/rs-talker`                  | Duplicate of `bsp-talker` (identical deps and source)   |
| `examples/qemu/rs-listener`                | Duplicate of `bsp-listener` (identical deps and source) |
| `packages/reference/embedded-cpp-talker`   | Depends on non-existent `nros-cpp` crate                |
| `packages/reference/embedded-cpp-listener` | Depends on non-existent `nros-cpp` crate                |

**Unchanged:**

| Path                                     | Reason                                 |
|------------------------------------------|----------------------------------------|
| `packages/reference/qemu-smoltcp-bridge` | Library (`src/lib.rs`), not an example |

**Work items:**
- [x] Move all 35 examples per the tables above (`git mv`)
- [x] Delete 4 dead/duplicate items
- [x] Update `Cargo.toml` path dependencies in each moved example (adjust `../` depth)
- [x] Update `.cargo/config.toml` `[patch.crates-io]` paths in each moved Rust example
- [x] Update `CMakeLists.txt` paths in moved C examples
- [x] Revise `justfile` recipes (see **Justfile Recipe Revision** below)
- [x] Update `CLAUDE.md` workspace structure tree
- [x] Update integration test fixtures referencing example binary paths (`nros-tests`)
- [x] Update doc references to example paths (`CLAUDE.md`, `tests/README.md`, phase docs)
- [x] Update `packages/reference/README.md` (remove moved entries, note library remains)
- [x] Run `just quality` after each platform group

**Passing criteria:**
- All moved examples build: `just build-examples`
- All tests pass: `just test`
- `just format-examples` discovers and formats all examples
- No references to old paths in `justfile`, `CLAUDE.md`, or test fixtures
- `packages/reference/` contains only `qemu-smoltcp-bridge` (library) and `README.md`

#### Justfile Recipe Revision

The current justfile uses 5 hardcoded example lists (lines 5–9) and per-platform recipes with different base directories. After the move to the 4-level `platform/language/rmw/use-case` structure, these must be revised.

**Current hardcoded lists (to be removed):**

| Variable                  | Current value                                    | Where used                                                                        |
|---------------------------|--------------------------------------------------|-----------------------------------------------------------------------------------|
| `NATIVE_EXAMPLES`         | `rs-talker rs-listener ...` (7 items)            | `build/format/check/clean-examples-native`                                        |
| `EMBEDDED_EXAMPLES`       | `stm32f4-rtic stm32f4-embassy ...` (4 items)     | `build/format/check/clean-examples-embedded` (base: `packages/reference/`)        |
| `QEMU_EXAMPLES`           | `qemu/rs-test qemu/rs-wcet-bench`                | `build/format/check/clean-examples-qemu`, `quality`                               |
| `QEMU_REFERENCE_EXAMPLES` | `qemu-smoltcp-bridge qemu-lan9118`               | `build/format/check/clean-examples-qemu`, `quality` (base: `packages/reference/`) |
| `QEMU_ZENOH_EXAMPLES`     | `qemu/rs-talker ... qemu/bsp-listener` (4 items) | `build/format/check/clean-examples-qemu`, `quality`                               |

**Strategy: Replace hardcoded lists with `find`-based auto-discovery.**

After the move, all Rust examples live under `examples/` at depth 4–5:
```
examples/{platform}/{language}/{rmw}/{use-case}/Cargo.toml
```

The new glob pattern is:
```bash
find examples -name Cargo.toml -mindepth 4
```

**Recipe changes:**

1. **Remove all 5 hardcoded variables** (`NATIVE_EXAMPLES`, `EMBEDDED_EXAMPLES`, `QEMU_EXAMPLES`, `QEMU_REFERENCE_EXAMPLES`, `QEMU_ZENOH_EXAMPLES`).

2. **`format-examples`** — already uses auto-discovery (`_format-examples-auto`). Update glob from `examples/*/*/Cargo.toml` to `find examples -name Cargo.toml -mindepth 4`. Remove `format-examples-embedded` dependency (embedded examples move into `examples/`). Remove the per-platform `format-examples-native`, `format-examples-embedded`, `format-examples-qemu` recipes.

3. **`check-examples`** — replace the chain of `check-examples-native check-examples-embedded check-examples-qemu` with a single auto-discovery recipe. Build mode (debug vs release) is determined by platform:
   - `native/` → `cargo clippy` (debug)
   - `qemu-arm/`, `qemu-esp32/`, `esp32/`, `stm32f4/` → `cargo clippy --release`
   - `zephyr/` → separate (built via `west`, not cargo clippy)
   ```bash
   for toml in $(find examples -name Cargo.toml -mindepth 4 -not -path '*/zephyr/*'); do
       dir="$(dirname "$toml")"
       platform="$(echo "$dir" | cut -d/ -f2)"
       flags=""
       if [ "$platform" != "native" ]; then flags="--release"; fi
       (cd "$dir" && cargo +nightly fmt --check && cargo clippy $flags -- $CLIPPY_LINTS)
   done
   ```

4. **`build-examples`** — same auto-discovery pattern as `check-examples`. Replace the chain of `build-examples-native build-examples-embedded build-examples-qemu`. Build mode varies by platform (same native=debug, embedded=release rule). Exclude `zephyr/` and C examples (built separately via `west`/`cmake`).

5. **`clean-examples`** — auto-discover and `rm -rf` target dirs:
   ```bash
   for toml in $(find examples -name Cargo.toml -mindepth 4); do
       rm -rf "$(dirname "$toml")/target"
   done
   ```

6. **`quality` recipe (QEMU examples section, lines 167–180)** — replace the hardcoded `QEMU_EXAMPLES`/`QEMU_ZENOH_EXAMPLES`/`QEMU_REFERENCE_EXAMPLES` loops with auto-discovery of `examples/qemu-arm/`:
   ```bash
   for toml in $(find examples/qemu-arm -name Cargo.toml -mindepth 3); do
       dir="$(dirname "$toml")"
       (cd "$dir" && cargo +nightly fmt --check && cargo clippy --release -- $CLIPPY_LINTS)
   done
   ```

7. **`size-examples-embedded`** — update binary paths from `packages/reference/stm32f4-*/target/...` to `examples/stm32f4/rust/*/target/...`:
   | Old path | New path |
   |---|---|
   | `packages/reference/stm32f4-rtic/target/.../stm32f4-rtic-example` | `examples/stm32f4/rust/zenoh/rtic/target/.../stm32f4-rtic-example` |
   | `packages/reference/stm32f4-embassy/target/.../stm32f4-embassy-example` | `examples/stm32f4/rust/core/embassy/target/.../stm32f4-embassy-example` |
   | `packages/reference/stm32f4-polling/target/.../stm32f4-polling-example` | `examples/stm32f4/rust/zenoh/polling/target/.../stm32f4-polling-example` |
   | `packages/reference/stm32f4-smoltcp/target/.../stm32f4-smoltcp` | `examples/stm32f4/rust/standalone/smoltcp/target/.../stm32f4-smoltcp` |

8. **QEMU test recipes** — update hardcoded `-kernel` paths:
   | Recipe              | Old `-kernel` path                                           | New `-kernel` path                                                     |
   |---------------------|--------------------------------------------------------------|------------------------------------------------------------------------|
   | `test-qemu-basic`   | `examples/qemu/rs-test/target/.../qemu-rs-test`              | `examples/qemu-arm/rust/core/cdr-test/target/.../qemu-rs-test`         |
   | `test-qemu-wcet`    | `examples/qemu/rs-wcet-bench/target/.../qemu-rs-wcet-bench`  | `examples/qemu-arm/rust/core/wcet-bench/target/.../qemu-rs-wcet-bench` |
   | `test-qemu-lan9118` | `packages/reference/qemu-lan9118/target/.../qemu-rs-lan9118` | `examples/qemu-arm/rust/standalone/lan9118/target/.../qemu-rs-lan9118` |

   Note: binary names (after last `/`) stay the same — they come from `[[bin]]` or `name` in each example's `Cargo.toml`.

9. **ESP32 build recipes** — update paths:
   | Recipe                      | Old pattern                                  | New pattern                                        |
   |-----------------------------|----------------------------------------------|----------------------------------------------------|
   | `build-examples-esp32`      | `examples/esp32/{bsp-talker,bsp-listener}`   | `examples/esp32/rust/zenoh/{talker,listener}`      |
   | `build-examples-esp32-qemu` | `examples/esp32/{qemu-talker,qemu-listener}` | `examples/qemu-esp32/rust/zenoh/{talker,listener}` |
   | `test-qemu-esp32-basic`     | `build/esp32-qemu/esp32-qemu-talker.bin`     | Same (build output path, not source path)          |

10. **Zephyr recipes** — paths stay as `zephyr-workspace/` managed by `west`. The `examples/zephyr/` source paths change but are referenced through the west manifest, not directly in justfile.

11. **C example recipes** — C examples (`native/c/zenoh/*`, `zephyr/c/zenoh/*`) are built by CMake, not cargo. Their justfile recipes (`test-c`, `build-zephyr-c`) reference CMakeLists.txt paths that need updating.

**Recipes to delete (absorbed into auto-discovery):**
- `format-examples-native`
- `format-examples-embedded`
- `format-examples-qemu`
- `check-examples-native`
- `check-examples-embedded`
- `check-examples-qemu`
- `build-examples-native`
- `build-examples-embedded`
- `clean-examples-native`
- `clean-examples-embedded`
- `clean-examples-qemu`

**Recipes to keep (platform-specific build logic):**
- `build-examples-qemu` (needs `--release` + specific target)
- `build-examples-esp32` / `build-examples-esp32-qemu` (needs `+nightly`, env vars)
- `build-zephyr` / `build-zephyr-c` (uses `west build`)
- All `test-qemu-*` recipes (hardcoded QEMU launch commands)
- `size-examples-embedded` (specific binary paths)

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
    zpico-platform-mps2-an385/             #   System symbols for QEMU (z_malloc, etc.)
    zpico-platform-esp32/            #   System symbols for ESP32 WiFi
    zpico-platform-esp32-qemu/       #   System symbols for ESP32 QEMU
    zpico-platform-stm32f4/          #   System symbols for STM32F4
    zpico-zephyr/                    #   Zephyr C convenience library
    nros-rmw-zenoh/                  #   RMW glue (bridges zpico ↔ nros-rmw)
  boards/                            # User-facing platform packages (nros deps)
    nros-mps2-an385/                       #   QEMU: Publisher<M>, run_node(), Config
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
