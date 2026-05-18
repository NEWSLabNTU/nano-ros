# Phase 23: Precompiled C Library for Arduino IDE

**Goal**: Ship a precompiled `libnanoros.a` per ESP32 variant plus an
Arduino IDE library wrapper (`arduino/nros/`) so Arduino users can
publish/subscribe ROS 2 topics from a sketch with no Rust toolchain,
no agent, and no `colcon` install — just `Library Manager → install
nros` and a sketch.

**Status**: In Progress (23.0 is the active blocker; 23.1 skeleton lands first).
**Priority**: Medium
**Depends on**: Phase 21 (reopened — `platform-esp-idf` for `nros-c`),
  Phase 142 (extended SDK tier covers ESP-IDF install),
  Phase 143 (patched qemu-system-arm — used for cross-arch interop tests),
  Phase 139 (`integrations/esp-idf/` shell — provides the
  `add_subdirectory(<nano-ros>)` glue Arduino library re-uses).

## Why this phase changed (2026-05-18 rewrite)

The original 2024 design assumed:

- `nros-c` had a working bare-metal C API backend ready to
  cross-compile for ESP32 (Phase 21 closure).
- ESP-IDF integration was untested but architecturally straightforward.
- The remaining work was packaging only.

The current build-system review shows:

1. `nros-c`'s platform feature set is
   `platform-{posix,zephyr,freertos,nuttx,threadx}`. There is no
   `platform-bare-metal` / `platform-esp-idf` axis, and
   `packages/core/nros-c/CMakeLists.txt:90` fatal-errors on
   `NANO_ROS_PLATFORM=baremetal`.
2. The Phase 139 ESP-IDF integration shell at
   `integrations/esp-idf/CMakeLists.txt:26` already sets
   `NANO_ROS_PLATFORM=baremetal`, so the integration has never
   compiled `nros-c` end to end.
3. CLAUDE.md "Examples = Standalone Projects" lists
   `esp32/{c,cpp}/*` as deliberately empty cells because
   "`nros-c`/`nros-cpp` assume hosted RTOS for startup/heap/libc"
   — the bare-metal C harness needed by Arduino does not exist yet.
4. The colcon-as-build-driver assumption was dropped (Phase 78
   archived 2026-05-18). Arduino users consume the precompiled `.a`
   directly via Arduino IDE; the build artefact lives in
   `arduino/nros/src/<arch>/` instead of an `install/` prefix.

Phase 21 is reopened (subphases 21.6–21.10) to land the ESP-IDF
backend; Phase 23 now treats that work as its 23.0 prerequisite and
focuses on packaging + Arduino-specific glue.

## Architecture (unchanged)

```
┌─────────────────────────────────────────────────────────────────┐
│                    Arduino IDE Sketch (.ino)                     │
│                                                                  │
│  #include <nros_arduino.h>                                       │
│  #include <std_msgs/msg/int32.h>                                 │
│                                                                  │
│  setup(): set_nanoros_wifi_transports(ssid, pass, locator);      │
│           nros_init(&ctx, ...);                                  │
│           nros_node_create(&node, &ctx, "talker");               │
│           nros_publisher_create(&pub, &node, "/chatter");        │
│                                                                  │
│  loop():  nros_publish(&pub, &msg, sizeof(msg));                 │
│           nros_spin_once(&ctx, timeout_ms);                      │
└────────────────────────────┬────────────────────────────────────┘
                             │ C API calls
┌────────────────────────────▼────────────────────────────────────┐
│              nros_arduino.h  (transport setup)                  │
│  set_nanoros_wifi_transports(ssid, pass, locator)               │
│  NRCHECK / NRSOFTCHECK macros                                   │
│  Arduino WiFi.h / Serial integration (~70 lines)                │
└────────────────────────────┬────────────────────────────────────┘
                             │
┌────────────────────────────▼────────────────────────────────────┐
│                 libnanoros.a (precompiled)                       │
│  libnros_c.a    (platform-esp-idf + rmw-cffi + zenoh)            │
│  libzpico.a     (zenoh-pico, cross-built via zpico-sys)          │
│  Platform layer (lwIP via ESP-IDF, smoltcp NOT required)         │
└─────────────────────────────────────────────────────────────────┘
```

Key updates from the 2024 design:

- The library uses **`platform-esp-idf`** (Phase 21.6) rather than
  hypothetical `shim-smoltcp`. arduino-esp32 ships ESP-IDF's lwIP +
  WiFi stack already; nros routes through that instead of bundling
  its own smoltcp instance.
- The library re-uses the Phase 137 / 140 / 144 cmake consumption
  shape (`add_subdirectory(<nano-ros>)` → `NanoRos::NanoRos`) for the
  cross-compile step. There is no `find_package(NanoRos)` path
  (deleted Phase 140).
- The library packaging script uses the Phase 142
  `tier=extended` setup orchestrator to install ESP-IDF on demand;
  contributors who only touch the wrapper can skip the toolchain
  install and run unit tests against the host C API.

## Implementation Plan

### 23.0 — Unblock `nros-c` for ESP-IDF (delegated to Phase 21.6–21.10)

**Status**: In Progress

`nros-c` must build with `cargo build -p nros-c --no-default-features
--features "platform-esp-idf,rmw-cffi,cffi-zenoh-cffi,ros-humble"`
against the Xtensa / RISC-V ESP-IDF toolchains. See Phase 21 for the
task list; Phase 23 cannot publish a `libnanoros.a` for ESP32 until
21.6–21.10 land.

**Verification gate**: `just esp_idf build target=esp32c3` must
produce a `libnros_c.a` that links against ESP-IDF's libc + FreeRTOS
without undefined-reference errors.

### 23.1 — Arduino Library Skeleton

**Status**: Landing (foundation only; populated as later subphases close)

**Tasks**:

- [ ] **23.1.1** Create `arduino/nros/library.properties` with
      `precompiled=true`, `architectures=esp32`, `version` synced from
      workspace tag.
- [ ] **23.1.2** Create `arduino/nros/keywords.txt` for Arduino IDE
      syntax highlighting on `nros_*` functions + `NRCHECK` /
      `NRSOFTCHECK` macros.
- [ ] **23.1.3** Create `arduino/nros/src/nros_arduino.h` /
      `nros_arduino.cpp` (~70 lines total) for
      `set_nanoros_wifi_transports()` glue. Mirrors micro-ROS's
      `set_microros_wifi_transports()` shape so Arduino users
      familiar with micro-ROS find the API immediately recognisable.
- [ ] **23.1.4** Bundle a curated set of message-type headers under
      `arduino/nros/src/<package>/` (`std_msgs`, `geometry_msgs`,
      `sensor_msgs` to start). Generated via
      `cargo nano-ros generate-c` at build time, checked in.
- [ ] **23.1.5** Empty per-arch `.a` slot directories
      (`arduino/nros/src/esp32/`, `…/esp32s3/`, `…/esp32c3/`) plus
      a `.gitignore` marker so the directories exist before the
      build script populates them.
- [ ] **23.1.6** `arduino/nros/README.md` covering install (Library
      Manager + manual zip), WiFi + zenohd setup, and the
      `set_nanoros_wifi_transports()` API.
- [ ] **23.1.7** `arduino/nros/examples/{Talker,Listener}/` minimal
      sketches that compile against the bundled headers.

### 23.2 — Precompilation Build System

**Status**: Not Started (blocked by 23.0)

**Goal**: Produce `arduino/nros/src/<arch>/libnanoros.a` for each
ESP32 chip, plus the message headers, with one `just` recipe.

**What goes into `libnanoros.a`** (per target):
```
libnanoros.a
├── libnros_c.a    ← cargo build -p nros-c --target <triple> --features platform-esp-idf …
└── libzpico.a     ← built by zpico-sys for the matching target
```

Bundled into a single `libnanoros.a` via `ar crsT` so Arduino sketches
need only `-lnanoros`.

**Targets**:

| Board                | Rust target                   | GCC toolchain      |
|----------------------|-------------------------------|--------------------|
| ESP32-C3 (RISC-V)    | `riscv32imc-unknown-none-elf` | `riscv32-esp-elf`  |
| ESP32-S3 (Xtensa)    | `xtensa-esp32s3-none-elf`     | `xtensa-esp-elf`   |
| ESP32 (Xtensa LX6)   | `xtensa-esp32-none-elf`       | `xtensa-esp-elf`   |

**Tasks**:

- [ ] **23.2.1** Create `scripts/arduino/build-libnanoros.sh` —
      per-target build driver. Internally runs the Phase 139 ESP-IDF
      integration shell + an `ar` step to bundle the `.a` files.
- [ ] **23.2.2** Create `scripts/arduino/package-arduino-lib.sh` to
      assemble the final zip (`nano-ros-arduino-v<version>.zip`).
- [ ] **23.2.3** `nm` smoke checks: `nm -g libnanoros.a | grep
      ' T nros_'` and `nm -u libnanoros.a` rejects any POSIX-only
      symbol (`pthread_*`, `dlopen`, etc.).
- [ ] **23.2.4** `just build-arduino-libs` + `just package-arduino`
      recipes.
- [ ] **23.2.5** GitHub Actions matrix: build per-arch `.a` on
      release tags, attach the zip as a Release asset.

### 23.3 — Arduino Transport Glue

**Status**: Lands incrementally with 23.1

The only Arduino-specific code is the transport setup (~70 lines).
Implementation lives entirely in `arduino/nros/src/nros_arduino.cpp`;
no changes to `nros-c` are needed beyond Phase 21's ESP-IDF backend.

- [ ] **23.3.1** `set_nanoros_wifi_transports(ssid, pass, locator)` —
      calls `WiFi.begin()`, awaits `WL_CONNECTED`, stores the zenoh
      locator for the subsequent `nros_init()`.
- [ ] **23.3.2** `nanoros_ping(timeout_ms)` connectivity helper using
      zenoh scout / session open + close.
- [ ] **23.3.3** `NRCHECK` / `NRSOFTCHECK` error macros (Serial.printf
      output).

### 23.4 — Example Sketches

- [ ] **23.4.1** `Talker.ino` — publish `std_msgs/Int32` every second.
- [ ] **23.4.2** `Listener.ino` — subscribe + Serial-print payload.
- [ ] **23.4.3** `ServiceClient.ino` — `example_interfaces/AddTwoInts`.
- [ ] **23.4.4** `Reconnection.ino` — recover from WiFi + zenohd
      disconnects via `nanoros_ping()`.

### 23.5 — Testing

Tiered: host-fast → emulator → hardware. The first two run in CI; the
last is manual.

- [ ] **23.5a** C API coverage audit: every `nros_*` function used by
      the example sketches has at least one case in
      `just test-c`.
- [ ] **23.5b** QEMU ESP32-C3 `libnanoros.a` integration test —
      minimal C program flashed into Espressif's QEMU fork, publishes
      5 Int32 messages through zenohd, native Rust listener verifies.
- [ ] **23.5c** Cross-arch interop: ESP32-C3 QEMU ↔ ARM Cortex-M3
      QEMU (both publishing through zenohd) using the patched
      `qemu-system-arm` from Phase 143.
- [ ] **23.5d** Host transport-glue test with a mock `WiFi.h` stub:
      runs `nros_arduino.cpp` against a native libc build of
      `libnros_c.a` so the transport-glue logic is covered without
      QEMU or hardware.
- [ ] **23.5e** Hardware E2E (manual): ESP32-C3 DevKitC + Arduino
      Nano ESP32 over real WiFi, both directions, plus an
      `rmw_zenoh`-bridged ROS 2 sanity run.

### 23.6 — Documentation & Distribution

- [ ] **23.6.1** `arduino/nros/README.md` (lands with 23.1.6).
- [ ] **23.6.2** `book/src/getting-started/arduino.md` — installation,
      WiFi + zenohd setup, troubleshooting (~5-minute quickstart).
- [ ] **23.6.3** Contributor docs: regeneration workflow for the
      message-header bundle + custom-message rebuilding via the
      Phase 126 `nros build` pipeline.
- [ ] **23.6.4** Arduino Library Manager submission (post-v1).

## Dependencies

```
Phase 137 / 138 / 144 (add_subdirectory consumption) ─┐
Phase 139 (integrations/esp-idf shell) ───────────────┤
Phase 142 (extended SDK tier — esp_idf install) ──────┤
Phase 143 (patched qemu-system-arm — interop tests) ──┤
                                                       ▼
Phase 21.6–21.10 (platform-esp-idf for nros-c) ─→ 23.0
                                                       │
                                                       ▼
                                          23.1 (skeleton) ─→ 23.3 (glue)
                                                       │
                                                       ▼
                                          23.2 (build/package)
                                                       │
                                                       ▼
                                          23.4 (examples) ─→ 23.5a/d (host tests)
                                                       │
                                                       ▼
                                          23.5b/c (emulator tests)
                                                       │
                                                       ▼
                                          23.5e (hardware) + 23.6 (docs/release)
```

## Risks & Mitigations

| Risk                                                    | Impact | Mitigation                                                            |
|---------------------------------------------------------|--------|-----------------------------------------------------------------------|
| Phase 21 ESP-IDF backend slips                          | High   | Track 21.6–21.10 explicitly; Arduino skeleton (23.1) is shippable independently as a "Rust toolchain still required" intermediate. |
| ESP-IDF FreeRTOS port behaves differently from upstream | Medium | Vendor-port FreeRTOS APIs (queues, semaphores) are stable; differences live in startup + scheduler hooks which `nros-platform-cffi` already abstracts. |
| arduino-esp32 core version drift                        | Medium | Pin tested versions in `library.properties` + CI. ESP-IDF version is the upstream contract.                       |
| Library size > Arduino's 1 MB sketch hint               | Medium | LTO + `--strip-all`; nros is structurally smaller than micro-ROS (no Agent, no Micro XRCE-DDS). Measure per-arch.  |
| Custom message rebuild requires Rust toolchain          | Low    | Phase 126 codegen accepts pre-generated C headers; ship `cargo nano-ros generate-c` instructions for power users. |
| Espressif QEMU not yet on `just qemu setup` orchestrator | Medium | Track under Phase 142 (extended tier already covers esp_idf — espressif QEMU may need its own tier slot).         |

## Comparison with `micro_ros_arduino`

| Feature                  | micro_ros_arduino                | nros Arduino                    |
|--------------------------|----------------------------------|---------------------------------|
| User-facing API          | Raw C (rcl/rclc)                 | Raw C (`nros_*`)                |
| C++ wrappers             | None                             | None                            |
| Transport setup          | `set_microros_wifi_transports`   | `set_nanoros_wifi_transports`   |
| Error macros             | `RCCHECK` / `RCSOFTCHECK`        | `NRCHECK` / `NRSOFTCHECK`       |
| Spin model               | `rclc_executor_spin_some()`      | `nros_spin_once()`              |
| Agent required           | Yes (micro-ROS Agent)            | No (direct to zenohd)           |
| Middleware               | Micro XRCE-DDS                   | zenoh-pico                      |
| ROS 2 path               | Agent → DDS bridge               | `rmw_zenoh`                     |
| Install size per board   | ~22 MB                           | ~2 MB target (measure in 23.2)  |
| `precompiled=true`       | Yes                              | Yes                             |

## Future Extensions

- Additional message-package bundles (action_msgs, nav_msgs, tf2_msgs).
- Docker-based library rebuild for custom message types.
- Arduino Library Manager submission.
- PlatformIO library variant (re-uses the same `.a` set).
- RP2040 / STM32 chips (RP2040 lacks WiFi; will need Serial transport
  first).
- Optional thin C++ wrapper for users who prefer OOP.
