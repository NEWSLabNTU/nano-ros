# Phase 23: Precompiled C Library for Arduino IDE

**Goal**: Ship a precompiled `libnanoros.a` per ESP32 variant plus an
Arduino IDE library wrapper (`arduino/nros/`) so Arduino users can
publish/subscribe ROS 2 topics from a sketch with no Rust toolchain,
no agent, and no `colcon` install — just `Library Manager → install
nros` and a sketch.

**Status**: In Progress (2026-05-18). ESP32-C3 end-to-end works.
The single-chip happy path is:

```
just esp_idf setup            # one-time ESP-IDF v5.3 install
just build-arduino-libs       # → arduino/nros/src/esp32c3/libnanoros.a (56 KB)
just test-arduino-symbols     # nm audit (≥ 50 public T nros_* symbols)
just test-arduino-qemu-boot   # qemu-system-riscv32 -machine esp32c3 boot
just test-arduino-transport   # host WiFi-mock pub/sub glue smoke
just package-arduino          # → build/arduino/nano-ros-arduino-v*.zip (80 KB)
```

Plus `.github/workflows/arduino-release.yml` wires the whole
sequence as a release-tag-triggered job. Remaining open:
`23.2.x` (esp-rs Xtensa toolchain → ESP32 + ESP32-S3 matrix
rows), `23.4.x` (sketch API reconciliation — sketches are
aspirational against a wrapper layer that has not landed yet),
`23.5c` (cross-arch interop ESP32-C3 QEMU ↔ ARM Cortex-M3 QEMU),
`23.5e` (hardware — manual only), `23.6.4` (Arduino Library
Manager submission, post-v1).
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

- [x] **23.1.1** `arduino/nros/library.properties` —
      `precompiled=true`, `architectures=esp32`, `ldflags=-lnanoros`.
- [x] **23.1.2** `arduino/nros/keywords.txt`.
- [x] **23.1.3** `arduino/nros/src/nros_arduino.{h,cpp}` — WiFi
      bring-up + locator stash + `NRCHECK` / `NRSOFTCHECK` macros.
- [ ] **23.1.4** Bundle a curated set of message-type headers under
      `arduino/nros/src/<package>/` via `cargo nano-ros generate-c`.
      Deferred to first Library Manager submission (23.6.4).
- [x] **23.1.5** Per-arch `.gitkeep` directories (esp32 / esp32s3 /
      esp32c3) + `.gitignore` for the produced `.a` artefacts.
- [x] **23.1.6** `arduino/nros/README.md`.
- [x] **23.1.7** `arduino/nros/examples/{Talker,Listener}/`. Phase
      23.4 added `ServiceClient` + `Reconnection` too.

### 23.2 — Precompilation Build System

**Status**: ESP32-C3 end-to-end working (2026-05-18). ESP32 + S3
Xtensa pending Phase 23.2.x esp-rs toolchain install.

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

- [x] **23.2.1** `scripts/arduino/build-libnanoros.sh` — two-pass
      IDF driver (reconfigure → source CFLAGS → build → `ar crsT`
      bundle).
- [x] **23.2.2** `scripts/arduino/package-arduino-lib.sh`.
- [x] **23.2.3** `nm` smoke checks via `just test-arduino-symbols`
      (asserts ≥ 50 public `T nros_*` symbols, rejects POSIX-only
      undefined refs like `pthread_*` / `dlopen` / `fork` /
      `exec[lv]`).
- [x] **23.2.4** `just build-arduino-libs` + `just package-arduino`.
- [x] **23.2.5** `.github/workflows/arduino-release.yml` —
      release-tag-triggered matrix per chip; runs
      `just esp_idf setup` + `just build-arduino-libs` + nm smoke +
      QEMU boot + `just package-arduino`; uploads the zip as a
      Release asset. ESP32 + ESP32-S3 matrix rows commented out
      until 23.2.x lands esp-rs Xtensa toolchain wiring.
- [ ] **23.2.x** Xtensa Rust toolchain — stock rustup lacks
      `xtensa-esp32{,s3}-none-elf`. Need `espup install` (or vendor
      the esp-rs channel under `esp-idf-workspace/`). Until that
      lands, `ARDUINO_LIB_TARGETS` defaults to `esp32c3` only.

### 23.3 — Arduino Transport Glue

**Status**: Lands incrementally with 23.1

The only Arduino-specific code is the transport setup (~70 lines).
Implementation lives entirely in `arduino/nros/src/nros_arduino.cpp`;
no changes to `nros-c` are needed beyond Phase 21's ESP-IDF backend.

- [x] **23.3.1** `set_nanoros_wifi_transports(ssid, pass, locator)`.
- [ ] **23.3.2** `nanoros_ping(timeout_ms)` — current implementation
      proxies `WiFi.status()`. Upgrade to a real zenoh scout / open+close
      cycle once the runtime API path is exercised under sketch
      conditions (gated by Phase 23.4.x sketch-API reconciliation).
- [x] **23.3.3** `NRCHECK` / `NRSOFTCHECK` error macros.

### 23.4 — Example Sketches

- [x] **23.4.1** `Talker.ino`.
- [x] **23.4.2** `Listener.ino`.
- [x] **23.4.3** `ServiceClient.ino` (calls AddTwoInts).
- [x] **23.4.4** `Reconnection.ino` (drives `nanoros_ping()` +
      bring-up / tear-down on failure).
- [ ] **23.4.x** Sketch API reconciliation. All four sketches use
      Arduino-shaped names (`nros_*_create` / `nros_spin_once`) that
      do NOT match nros-c's `_init` / `_fini` / `executor_init` +
      `executor_add_*` + `executor_spin_some` surface. Two
      paths investigated:
      - **(a) rewrite sketches against the real API** — concrete but
        loses the micro-ROS-shape ergonomics that Arduino users
        expect. Adds boilerplate for the executor object that
        micro-ROS hides.
      - **(b) thin Arduino-shape wrappers in `nros_arduino.h`** —
        wraps `nros_support_init` / `nros_node_init` /
        `nros_publisher_init` / `nros_publish_raw` /
        `nros_client_init` / `nros_client_call` plus a hidden global
        `nros_executor_t` so `nros_spin_once(&ctx, timeout)` resolves
        without the user constructing one explicitly. Closer to
        micro-ROS DX. Requires the bundled
        `arduino/nros/src/nros/` headers (landed by 23.1.4 follow-up
        in 2026-05) so the struct sizes resolve.
      Recommended path: **(b)**. Tracked separately because the
      wrapper layer needs care around executor lifetime, subscription
      callback signature reshape, and per-sketch resource sizing —
      not blocking the precompiled-library packaging.

### 23.5 — Testing

Tiered: host-fast → emulator → hardware. The first two run in CI; the
last is manual.

- [x] **23.5a** C API coverage audit completed 2026-05-18. Sketches
      under `arduino/nros/examples/` reference these
      `nros_*` symbols:
      `nros_init` / `nros_fini` / `nros_node_create` /
      `nros_node_destroy` / `nros_publisher_create` /
      `nros_publisher_destroy` / `nros_publish` /
      `nros_subscription_create` / `nros_client_create` /
      `nros_client_call` / `nros_spin_once`. The actual nros-c
      surface uses different names —
      `nros_support_init` / `nros_node_init` / `nros_publisher_init`
      / `nros_client_init` / `nros_executor_init` /
      `nros_executor_add_client` / `nros_executor_spin` etc.
      (see `examples/zephyr/c/zenoh/talker/src/main.c` for the
      canonical shape). Audit outcome: sketches are
      *aspirational* Arduino-shaped API; they must either
      (a) be rewritten against the real nros-c surface or
      (b) wrap the real surface under a thin Arduino-friendly
      shim in `arduino/nros/src/nros_arduino.h` (the `create` /
      `destroy` / `spin_once` shape mirrors micro-ROS more
      cleanly than the upstream rcl-style `init` / `fini` shape).
      Tracked as a follow-up Phase 23.4.x — does NOT block the
      precompiled-library packaging.
- [x] **23.5b** QEMU ESP32-C3 boot smoke (`just test-arduino-qemu-boot`):
      builds the merged flash image from `scripts/arduino/idf-builder/`
      (which links the per-arch `libnanoros.a`), boots it under
      `qemu-system-riscv32 -machine esp32c3`, asserts the placeholder
      `app_main` line prints. Proves every nano-ros symbol resolves at
      IDF link time without TAP / zenohd. Full pub-via-zenohd
      verification is gated by Phase 23.4.x sketch-API reconciliation
      (placeholder `app_main` is what currently runs — no `nros_*`
      calls).
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

- [x] **23.6.1** `arduino/nros/README.md` (landed with 23.1.6).
- [x] **23.6.2** `book/src/getting-started/arduino.md`.
- [x] **23.6.3** Contributor regen flow + custom-message workflow
      documented in the same book page.
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
