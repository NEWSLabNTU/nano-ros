# Phase 23: Precompiled C Library for Arduino IDE

**Goal**: Provide a precompiled nros Arduino library that enables Arduino IDE users to publish/subscribe ROS 2 topics using a C API with transport setup helpers — no Rust toolchain required.

**Status**: Not Started
**Priority**: Medium
**Depends on**: Phase 21 (C API `no_std` backend), Phase 22 (ESP32 support), Phase 11 (C API)

## Overview

Arduino is the most widely used embedded development platform, especially in education and hobbyist communities. By providing a precompiled Arduino library, nros can reach users who would never install a Rust toolchain.

This follows the same approach as micro-ROS Arduino (`micro_ros_arduino`): precompile `libnanoros.a` for each supported board, provide C API headers, and distribute as an Arduino Library. Like micro-ROS, users call the C API directly from their sketches — no C++ wrapper classes.

### Reference: micro_ros_arduino

The `micro_ros_arduino` library (source in `external/micro_ros_arduino/`) is the primary design reference. Key patterns we adopt:

1. **Raw C API, no C++ wrappers**: micro-ROS exposes `rcl`/`rclc` C functions directly. Users declare C structs (`rcl_node_t`, `rcl_publisher_t`) as globals and call `rclc_*()` functions. This is simpler to maintain and avoids ABI issues with C++ name mangling across compiler versions.

2. **Transport setup functions**: The only Arduino-specific abstraction is transport initialization:
   - `set_microros_transports()` — Serial (default)
   - `set_microros_wifi_transports(ssid, pass, agent_ip, port)` — WiFi UDP
   - Each is ~70 lines of Arduino-specific code calling a generic `rmw_uros_set_custom_transport()` with 4 callbacks (open/close/read/write).

3. **Executor-based spin model**: `rclc_executor_t` with pre-allocated handle slots. `rclc_executor_spin_some()` called in `loop()`.

4. **Pre-generated message types**: All message C structs and type-support introspection are bundled. Users `#include <std_msgs/msg/int32.h>` and use `ROSIDL_GET_MSG_TYPE_SUPPORT()` macro. Custom types require library rebuild.

5. **Error-handling macros**: `RCCHECK(fn)` / `RCSOFTCHECK(fn)` wrap every API call — a pattern we should replicate.

6. **`precompiled=true` in `library.properties`**: Arduino IDE links the correct `libmicroros.a` from `src/<architecture>/` based on the selected board.

**Key difference**: micro-ROS uses Micro XRCE-DDS (requires a host agent process). nros uses zenoh-pico (connects directly to zenohd, compatible with rmw_zenoh). This eliminates the agent, reduces latency, and simplifies the setup for end users.

### Key Advantage Over micro-ROS Arduino

| Aspect           | micro-ROS Arduino             | nros Arduino       |
|------------------|-------------------------------|------------------------|
| Agent required   | Yes (micro-ROS Agent on host) | **No** (direct zenoh)  |
| Network          | Serial to Agent (typically)   | WiFi direct to zenohd  |
| Setup complexity | High (Agent + ROS 2 install)  | **Low** (just zenohd)  |
| Latency          | Higher (Agent relay)          | **Lower** (direct)     |
| Library size     | ~22 MB per board              | ~2 MB (estimate)       |
| Board support    | Many (precompiled per board)  | ESP32 family initially |

### Target Boards (Initial)

| Board                       | Chip               | Arduino Core  | Priority |
|-----------------------------|--------------------|---------------|----------|
| Arduino Nano ESP32          | ESP32-S3           | arduino-esp32 | High     |
| ESP32-C3-DevKitC            | ESP32-C3           | arduino-esp32 | High     |
| ESP32-DevKitC               | ESP32              | arduino-esp32 | Medium   |
| Arduino Nano RP2040 Connect | RP2040 + Nina W102 | arduino-mbed  | Future   |

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                    Arduino IDE Sketch (.ino)                     │
│                                                                  │
│  #include <nano_ros_arduino.h>                                  │
│  #include <std_msgs/msg/int32.h>                                │
│                                                                  │
│  nano_ros_context_t ctx;                                        │
│  nros_node_t node;                                          │
│  nano_ros_publisher_t pub;                                      │
│  std_msgs__msg__Int32 msg;                                      │
│                                                                  │
│  setup(): set_nanoros_wifi_transports(ssid, pass, locator);     │
│           nano_ros_init(&ctx, ...);                             │
│           nros_node_create(&node, &ctx, "talker");          │
│           nano_ros_publisher_create(&pub, &node, "/chatter");   │
│                                                                  │
│  loop():  nano_ros_publish(&pub, &msg, sizeof(msg));            │
│           nano_ros_spin_once(&ctx, timeout_ms);                 │
└────────────────────────────┬────────────────────────────────────┘
                             │  C API calls
┌────────────────────────────▼────────────────────────────────────┐
│              nano_ros_arduino.h  (transport setup)              │
│                                                                  │
│  set_nanoros_wifi_transports(ssid, pass, locator)               │
│  set_nanoros_serial_transports()          (future)              │
│  NRCHECK(fn) / NRSOFTCHECK(fn) macros                          │
│  Arduino WiFi.h / Serial integration (~70 lines)               │
└────────────────────────────┬────────────────────────────────────┘
                             │
┌────────────────────────────▼────────────────────────────────────┐
│                 libnanoros.a (precompiled)                       │
│                                                                  │
│  nros-c     (C API: init, node, pub, sub, service, action) │
│  zenoh-pico     (transport + session management)                │
│  Platform layer (smoltcp or lwIP)                               │
└─────────────────────────────────────────────────────────────────┘
```

Unlike micro-ROS's 3-layer stack (rclc → rcl → rmw → Micro XRCE-DDS), nros has just 2 layers: the C API (which maps directly to zenoh-pico sessions/publishers/subscribers) and the transport setup glue. This makes the precompiled library significantly smaller.

### Build Pipeline

```
┌─────────────────────────────────────────────────────────────────┐
│                    Build Server / CI                             │
│                                                                  │
│  For each target board:                                         │
│  1. Cross-compile zenoh-pico → libzenohpico.a                  │
│  2. Cross-compile nros C shim → libnanoros_shim.a           │
│  3. Bundle: libnanoros.a = libzenohpico.a + libnanoros_shim.a   │
│  4. Copy headers from packages/core/nros-c/include/               │
│  5. Package into Arduino library structure                      │
│                                                                  │
│  Output per board:                                              │
│    src/<board>/libnanoros.a                                      │
│    src/nros.h                                               │
└─────────────────────────────────────────────────────────────────┘
```

## Target API

Following the micro-ROS pattern: raw C API with transport setup helpers and error-checking macros. No C++ wrapper classes — users call `nano_ros_*()` functions directly.

### Arduino Sketch (Talker)

```cpp
#include <nano_ros_arduino.h>
#include <nros/init.h>
#include <nros/node.h>
#include <nros/publisher.h>
#include <std_msgs/msg/int32.h>

// Error-handling macros (same pattern as micro-ROS)
#define NRCHECK(fn) { int rc = fn; if (rc != 0) { Serial.printf("Error %d at %s:%d\n", rc, __FILE__, __LINE__); while(1) delay(1000); }}
#define NRSOFTCHECK(fn) { int rc = fn; if (rc != 0) { Serial.printf("Warning %d at %s:%d\n", rc, __FILE__, __LINE__); }}

nano_ros_context_t ctx;
nros_node_t node;
nano_ros_publisher_t pub;
int count = 0;

void setup() {
    Serial.begin(115200);

    // Transport setup — the only Arduino-specific call
    set_nanoros_wifi_transports("MyNetwork", "password123", "tcp/192.168.1.1:7447");

    // Standard C API calls (same on all platforms)
    NRCHECK(nano_ros_init(&ctx));
    NRCHECK(nros_node_create(&node, &ctx, "talker"));
    NRCHECK(nano_ros_publisher_create(&pub, &node, "/chatter",
        NANO_ROS_MSG_TYPE_SUPPORT(std_msgs, msg, Int32)));
}

void loop() {
    std_msgs__msg__Int32 msg = { .data = count++ };
    NRSOFTCHECK(nano_ros_publish(&pub, &msg, sizeof(msg)));
    Serial.printf("Published: %d\n", msg.data);

    nano_ros_spin_once(&ctx, 100);
    delay(1000);
}
```

### Arduino Sketch (Listener)

```cpp
#include <nano_ros_arduino.h>
#include <nros/init.h>
#include <nros/node.h>
#include <nros/subscription.h>
#include <std_msgs/msg/int32.h>

#define NRCHECK(fn) { int rc = fn; if (rc != 0) { Serial.printf("Error %d\n", rc); while(1) delay(1000); }}

nano_ros_context_t ctx;
nros_node_t node;
nano_ros_subscription_t sub;
std_msgs__msg__Int32 msg;

void message_callback(const void* data, size_t len, void* user_data) {
    const std_msgs__msg__Int32* m = (const std_msgs__msg__Int32*)data;
    Serial.printf("Received: %d\n", m->data);
}

void setup() {
    Serial.begin(115200);
    set_nanoros_wifi_transports("MyNetwork", "password123", "tcp/192.168.1.1:7447");

    NRCHECK(nano_ros_init(&ctx));
    NRCHECK(nros_node_create(&node, &ctx, "listener"));
    NRCHECK(nano_ros_subscription_create(&sub, &node, "/chatter",
        NANO_ROS_MSG_TYPE_SUPPORT(std_msgs, msg, Int32),
        message_callback, NULL));
}

void loop() {
    nano_ros_spin_once(&ctx, 100);
}
```

### API Design Notes

- **`set_nanoros_wifi_transports()`** handles WiFi connection + zenoh locator configuration in one call, following micro-ROS's `set_microros_wifi_transports()` pattern. Internally calls `WiFi.begin()` and sets the zenoh locator for subsequent `nano_ros_init()`.
- **`NANO_ROS_MSG_TYPE_SUPPORT()`** macro provides type support metadata (topic type string, CDR size). This is used for ROS 2 topic type matching via rmw_zenoh.
- **`nano_ros_spin_once()`** polls zenoh-pico for incoming data and dispatches subscription callbacks. Unlike micro-ROS's executor model (which requires pre-allocating handle slots), nros dispatches directly from the zenoh session — no executor initialization step.
- **Error codes**: All `nano_ros_*()` functions return `int` (0 = success, negative = error). The `NRCHECK`/`NRSOFTCHECK` macros mirror micro-ROS's `RCCHECK`/`RCSOFTCHECK`.

## Implementation Plan

### 23.1: Arduino Library Structure

**Status**: Not Started

**Tasks**:
1. [ ] Create Arduino library directory:
   ```
   arduino/nros/
   ├── library.properties        # Arduino library metadata (precompiled=true)
   ├── keywords.txt              # Syntax highlighting for nano_ros_* functions
   ├── src/
   │   ├── nano_ros_arduino.h    # Transport setup (WiFi/Serial) + helper macros
   │   ├── nano_ros_arduino.cpp  # Transport setup implementation (~70 lines)
   │   ├── nros/             # C API headers (from packages/core/nros-c/include/)
   │   │   ├── init.h
   │   │   ├── node.h
   │   │   ├── publisher.h
   │   │   ├── subscription.h
   │   │   └── ...
   │   ├── std_msgs/             # Pre-generated message type headers
   │   │   └── msg/
   │   │       ├── int32.h
   │   │       ├── string.h
   │   │       └── ...
   │   ├── esp32s3/
   │   │   └── libnanoros.a      # Precompiled for ESP32-S3 (Xtensa)
   │   ├── esp32c3/
   │   │   └── libnanoros.a      # Precompiled for ESP32-C3 (RISC-V)
   │   └── esp32/
   │       └── libnanoros.a      # Precompiled for ESP32 (Xtensa)
   ├── examples/
   │   ├── Talker/
   │   │   └── Talker.ino
   │   ├── Listener/
   │   │   └── Listener.ino
   │   ├── ServiceClient/
   │   │   └── ServiceClient.ino
   │   └── Reconnection/
   │       └── Reconnection.ino
   └── README.md
   ```
2. [ ] Create `library.properties` with `precompiled=true` and `architectures=esp32`
3. [ ] Create `keywords.txt` for IDE syntax highlighting (`nano_ros_*` functions, `NRCHECK` macro)
4. [ ] Bundle pre-generated message headers for common types (std_msgs, geometry_msgs, sensor_msgs — following micro-ROS's approach of bundling ~58 packages)

**Acceptance Criteria**:
- [ ] Library structure follows Arduino Library Specification 2.2
- [ ] `precompiled=true` set in `library.properties`
- [ ] Arduino IDE auto-selects correct `libnanoros.a` for the target board

### 23.2: Precompilation Build System

**Status**: Not Started

**Goal**: Produce a self-contained Arduino library package containing precompiled `libnanoros.a` for each target board plus all necessary C headers — ready to drop into the Arduino library structure from 23.1.

**What goes into `libnanoros.a`**:
```
libnanoros.a (per target)
├── libnano_ros_c.a    ← cargo build -p nros-c --target <triple> --release
└── libzenohpico.a     ← cross-compiled C library from scripts/esp32/build-zenoh-pico.sh
```

Both archives are bundled into a single `libnanoros.a` via `ar` so Arduino sketches only need `-lnanoros`.

**Target triples**:

| Board            | Rust target                   | GCC target                                 |
|------------------|-------------------------------|--------------------------------------------|
| ESP32-C3         | `riscv32imc-unknown-none-elf` | `riscv64-unknown-elf` or `riscv32-esp-elf` |
| ESP32-S3         | `xtensa-esp32s3-none-elf`     | `xtensa-esp-elf`                           |
| ESP32 (original) | `xtensa-esp32-none-elf`       | `xtensa-esp-elf`                           |

**Tasks**:
1. [ ] Create `scripts/arduino/build-libnanoros.sh` — per-target build script:
   - Cross-compile zenoh-pico for the target (reuse `scripts/esp32/build-zenoh-pico.sh` pattern)
   - Cross-compile `nros-c` crate: `cargo build -p nros-c --target <triple> --release` with appropriate platform feature (not `shim-posix` — needs bare-metal/smoltcp backend from Phase 21)
   - Bundle both `.a` files: `ar crsT libnanoros.a libnano_ros_c.a libzenohpico.a`
   - Strip debug symbols: `strip --strip-debug libnanoros.a`
   - Output to `build/arduino/<board>/libnanoros.a`
2. [ ] Create `scripts/arduino/package-arduino-lib.sh` — assemble the Arduino library:
   - Copy `libnanoros.a` for each board into `arduino/nros/src/<board>/`
   - Copy C headers from `packages/core/nros-c/include/nros/` into `arduino/nros/src/nros/`
   - Copy transport setup files (`nano_ros_arduino.h`, `nano_ros_arduino.cpp`) into `arduino/nros/src/`
   - Copy example sketches into `arduino/nros/examples/`
   - Stamp version in `library.properties`
   - Produce distributable zip: `nano-ros-arduino-v<version>.zip`
3. [ ] Verify exported symbols: `nm -g libnanoros.a | grep ' T nano_ros_'` — all C API functions must be present
4. [ ] Verify no undefined platform symbols: `nm -u libnanoros.a` — must not reference POSIX APIs (`pthread_*`, `dlopen`, etc.)
5. [ ] Add `just build-arduino-libs` recipe (calls build-libnanoros.sh for each target)
6. [ ] Add `just package-arduino` recipe (calls package-arduino-lib.sh)
7. [ ] Test linking: compile a minimal C file against `libnanoros.a` + headers using the Arduino ESP32 core's GCC toolchain
8. [ ] Create CI workflow: build + package on release tags, upload zip as GitHub Release asset

**Open question**: `nros-c` currently requires `shim-posix` (POSIX sockets). For ESP32 bare-metal, it needs a platform backend that uses zenoh-pico's smoltcp integration (Phase 21). Until Phase 21 delivers a `shim-smoltcp` or `shim-esp32` feature, the cross-compilation in step 1 will fail at link time. This is the critical dependency.

**Acceptance Criteria**:
- [ ] `libnanoros.a` built for ESP32-C3 (RISC-V) — primary target
- [ ] `libnanoros.a` built for ESP32-S3 and ESP32 (Xtensa) — secondary
- [ ] Headers copied verbatim from `packages/core/nros-c/include/` (no cbindgen needed — headers are manually maintained)
- [ ] Distributable zip produced with correct Arduino library structure
- [ ] Arduino IDE can compile sketches linking the library
- [ ] `nm` verification passes (symbols present, no POSIX undefined refs)

### 23.3: Transport Setup and Arduino Glue

**Status**: Not Started

Following micro-ROS's pattern, the only Arduino-specific code is the transport setup (~70 lines per transport). The C API itself is platform-agnostic.

**Tasks**:
1. [ ] Implement `nano_ros_arduino.h` header:
   ```cpp
   #ifndef NANO_ROS_ARDUINO_H
   #define NANO_ROS_ARDUINO_H

   #include <nros/init.h>

   // Error-handling macros (same pattern as micro-ROS RCCHECK/RCSOFTCHECK)
   #define NRCHECK(fn) { int rc = fn; if (rc != 0) { \
       Serial.printf("[nros] Error %d at %s:%d\n", rc, __FILE__, __LINE__); \
       while(1) delay(1000); }}
   #define NRSOFTCHECK(fn) { int rc = fn; if (rc != 0) { \
       Serial.printf("[nros] Warning %d at %s:%d\n", rc, __FILE__, __LINE__); }}

   // Transport setup functions
   void set_nanoros_wifi_transports(const char* ssid, const char* pass,
                                     const char* zenoh_locator);
   void set_nanoros_serial_transports();  // Future: Serial transport

   // Utility
   bool nanoros_ping(uint32_t timeout_ms);  // Check if zenohd is reachable

   #endif
   ```
2. [ ] Implement `nano_ros_arduino.cpp`:
   - `set_nanoros_wifi_transports()`: Call `WiFi.begin(ssid, pass)`, wait for `WL_CONNECTED`, configure zenoh locator for subsequent `nano_ros_init()` call
   - `nanoros_ping()`: Lightweight connectivity check (zenoh scout or session open/close)
   - Platform detection via preprocessor (`#if defined(ESP32)`, `#elif defined(ARDUINO_ARCH_RP2040)`)
3. [ ] Implement `set_nanoros_serial_transports()` (future, lower priority — nros's value is direct WiFi/Ethernet without an agent)
4. [ ] Define error-handling macros (`NRCHECK`, `NRSOFTCHECK`)
5. [ ] Add `NANO_ROS_MSG_TYPE_SUPPORT()` macro in C API headers for typed topic creation

**Design rationale** (why no C++ wrapper classes):
- micro-ROS has no C++ wrappers and is widely adopted — Arduino users are comfortable with C structs + function calls
- C++ wrappers add ABI fragility across Arduino core versions and compiler updates
- Raw C API is easier to test (23.5a) and debug
- One less layer to maintain; the C API surface is already small
- Users who want OOP can write their own thin wrappers around the C API

**Acceptance Criteria**:
- [ ] Transport setup compiles with Arduino ESP32 core (arduino-esp32 v2.x and v3.x)
- [ ] WiFi transport connects and configures zenoh locator
- [ ] Error messages printed to Serial with file/line info
- [ ] `nanoros_ping()` correctly detects zenohd availability

### 23.4: Example Sketches

**Status**: Not Started

All examples follow the micro-ROS convention: globals for handles, `setup()` for init, `loop()` for spin. Each example includes WiFi configuration as user-editable constants at the top.

**Tasks**:
1. [ ] Create `Talker.ino` — publish `std_msgs/Int32` every second (see Target API above)
2. [ ] Create `Listener.ino` — subscribe to topic, print received messages
3. [ ] Create `ServiceClient.ino` — call AddTwoInts service, print result
4. [ ] Create `Reconnection.ino` — reconnect on zenohd disconnect using `nanoros_ping()` (modeled on micro-ROS's `micro-ros_reconnection_example.ino`)
5. [ ] Each example has user-editable WiFi/locator constants at the top
6. [ ] Test all examples on Arduino Nano ESP32

**Acceptance Criteria**:
- [ ] Examples compile in Arduino IDE without modification (after WiFi credentials)
- [ ] Examples work out-of-the-box on Arduino Nano ESP32
- [ ] Each example includes clear comments explaining each API call
- [ ] Reconnection example handles WiFi and zenohd disconnects gracefully

### 23.5: Testing

**Status**: Not Started

Testing is organized into tiers, from fastest/cheapest (host-native, no hardware) to slowest/most expensive (physical WiFi boards). Emulator-based tests run in CI; hardware tests are manual.

#### 23.5a: C API Coverage for Arduino Entry Points

**Depends on**: 23.3 (transport setup, to know which C functions the examples use)

Verify the existing C API test suite (`just test-c`) covers every `nano_ros_*` function used by the example sketches. No new infrastructure needed — this runs on the host against the native `libnano_ros_c.a`.

**Tasks**:
1. [ ] Audit example sketches → list every `nano_ros_*` C function they call
2. [ ] Cross-reference with `packages/testing/nros-tests/tests/c_api.rs` — flag gaps
3. [ ] Add missing C API test cases (e.g., if `nano_ros_spin_once` or subscription callbacks aren't covered)

**Acceptance Criteria**:
- [ ] Every C API function used by the Arduino examples has at least one test in `test-c`

#### 23.5b: QEMU ESP32-C3 `libnanoros.a` Integration Test

**Depends on**: 22.5c (Espressif QEMU installed), 22.5d (QEMU interop infra), 23.2 (precompiled library)

Test the precompiled `libnanoros.a` on the actual RISC-V target — in QEMU, not on hardware. Build a minimal C test program that links `libnanoros.a`, publishes a message, and verifies it arrives at a native listener via zenohd. Uses OpenETH (not WiFi) for networking.

**Architecture**:
```
┌──────────────────┐         ┌─────────┐         ┌───────────────┐
│ QEMU ESP32-C3    │  TAP    │ zenohd  │         │ native        │
│  C test program  │◄───────►│ (host)  │◄───────►│ rs-listener   │
│  links libnanoros│  eth    │         │  tcp    │               │
│  OpenETH + smol  │         │         │         │               │
└──────────────────┘         └─────────┘         └───────────────┘
```

**Tasks**:
1. [ ] Create `tests/arduino/test-libnanoros-esp32c3.c` — minimal C program:
   - `#include <nros/init.h>`, `<nros/publisher.h>`, etc.
   - Open session, create publisher, publish 5 CDR-encoded Int32 messages, close
   - Print `[PASS]` / `[FAIL]` markers for semihosting/UART capture
2. [ ] Create build script: compile test program + link `libnanoros.a` for RISC-V, produce flash image via `espflash save-image`
3. [ ] Create Docker Compose config for ESP32-C3 QEMU tests (zenohd + QEMU ESP32-C3 + native listener)
4. [ ] Add `esp32_arduino.rs` test suite in `packages/testing/nros-tests/tests/` (or extend `esp32_emulator.rs` from 22.5d)
5. [ ] Add `just test-qemu-esp32-arduino` recipe

**Acceptance Criteria**:
- [ ] C test program using only `libnanoros.a` + headers successfully publishes from QEMU ESP32-C3
- [ ] Native listener receives messages (verified by Docker E2E test)
- [ ] Runs in CI without physical hardware

#### 23.5c: Cross-Architecture Interop

**Depends on**: 23.5b, existing ARM QEMU infra

Validate wire-compatibility of `libnanoros.a` output across architectures. This catches endianness, alignment, or CDR encoding bugs that only surface when different targets communicate.

**Tasks**:
1. [ ] Test: ESP32-C3 QEMU talker → ARM Cortex-M3 QEMU listener (via zenohd)
2. [ ] Test: ARM Cortex-M3 QEMU talker → ESP32-C3 QEMU listener (via zenohd)
3. [ ] Test: ESP32-C3 QEMU talker → native `rs-listener` (Rust, via zenohd)
4. [ ] Add Docker Compose config with three architectures (zenohd + ESP32-C3 QEMU + ARM QEMU)

**Acceptance Criteria**:
- [ ] Bidirectional cross-architecture pub/sub works
- [ ] Tests run in CI

#### 23.5d: Host-Native Transport Setup Test (Mock WiFi)

**Depends on**: 23.3 (transport setup)

Test `nano_ros_arduino.cpp` on x86 Linux without any emulator or hardware. Compile the transport setup code against the native `libnano_ros_c.a` with a stub `WiFi.h` that returns "connected" immediately. This validates the transport setup logic and `nanoros_ping()` without needing WiFi or an ESP32.

**Tasks**:
1. [ ] Create `tests/arduino/mock_wifi/WiFi.h` — stub that satisfies `WiFi.begin()`, `WiFi.status()` with no-ops
2. [ ] Create `tests/arduino/test-transport-host.cpp` — call `set_nanoros_wifi_transports()` with mock WiFi, then `nano_ros_init()` → `nros_node_create()` → publish/subscribe via zenohd on localhost
3. [ ] Add CMake build for host transport test (link `nano_ros_arduino.cpp` + `libnano_ros_c.a` + stubs)
4. [ ] Add `just test-arduino-transport` recipe

**Acceptance Criteria**:
- [ ] Transport setup compiles and runs on x86 Linux with mock WiFi
- [ ] Pub/sub round-trip through zenohd works via the C API with transport setup
- [ ] Runs in CI

#### 23.5e: Hardware WiFi E2E Tests

**Depends on**: 23.4 (example sketches), physical ESP32 board

Manual tests on real hardware. These validate WiFi connectivity, real-world latency, and the actual Arduino IDE compilation flow. Not CI-able.

**Tasks**:
1. [ ] Test Arduino Talker (ESP32 WiFi) ↔ native Listener (via zenohd)
2. [ ] Test native Talker ↔ Arduino Listener (ESP32 WiFi, via zenohd)
3. [ ] Test Arduino ↔ ROS 2 interop (via rmw_zenoh)
4. [ ] Measure WiFi pub/sub latency (round-trip)
5. [ ] Verify on at least 2 boards: ESP32-C3-DevKitC + Arduino Nano ESP32

**Acceptance Criteria**:
- [ ] Bidirectional communication over WiFi works
- [ ] ROS 2 interop verified
- [ ] Tested on at least 2 ESP32 boards

### 23.6: Documentation

**Status**: Not Started

**Tasks**:
1. [ ] Create `arduino/nros/README.md` with:
   - Installation instructions (Arduino Library Manager or manual zip)
   - Quick start guide (WiFi + zenohd setup)
   - Troubleshooting (WiFi issues, library size, board selection)
2. [ ] Create `docs/arduino-setup.md` with detailed guide
3. [ ] Document library rebuild process (for contributors)
4. [ ] Consider Arduino Library Manager submission (future)

**Acceptance Criteria**:
- [ ] README enables new users to get running in <15 minutes

## Dependencies

```
Phase 11 (C API) ───────────┐
                              │
Phase 21 (C API no_std) ────┤   ← libnanoros.a needs bare-metal platform backend
                              │
Phase 22.2 (zenoh-pico       │
  cross-compile) ────────────┤
                              │
Phase 22.5c/d (ESP32-C3      │
  QEMU infra) ───────────────┤
                              ▼
23.1 (Library structure) ────┤
                              │
23.2 (Build + package) ──────┤
           │                  │
           │                  ▼
           │         23.3 (Transport setup) ────────────────┐
           │                  │                              │
           │                  ▼                              ▼
           │         23.4 (Examples)               23.5a (C API coverage)
           │                  │                              │
           ▼                  │                              ▼
  23.5b (QEMU ESP32-C3       │                     23.5d (Host transport test)
    libnanoros.a test)        │
           │                  │
           ▼                  ▼
  23.5c (Cross-arch     23.5e (HW WiFi E2E)
    interop)                  │
           │                  │
           ▼                  ▼
                      23.6 (Documentation)
```

**Key dependency notes**:
- **Phase 21** (C API `no_std` backend) is the critical blocker: `nros-c` currently requires `shim-posix` (POSIX sockets). Cross-compiling for ESP32 bare-metal needs a `shim-smoltcp` or equivalent platform backend that routes through zenoh-pico's smoltcp integration.
- **Phase 22.2** provides the zenoh-pico RISC-V cross-compilation scripts reused by 23.2.
- **Phase 22.5c/d** provides the Espressif QEMU installation and test infrastructure needed for 23.5b emulator tests.
- **Phase 11** (C API) provides the `nano_ros_*` functions and headers that the Arduino wrapper calls.
- **23.5b and 23.5c** (emulator tests) can proceed independently of **23.5d and 23.5e** (wrapper/WiFi tests).

## Risks and Mitigations

| Risk                                       | Impact | Mitigation                                   |
|--------------------------------------------|--------|----------------------------------------------|
| Phase 21 `no_std` C API backend not ready  | **High** | This blocks cross-compilation entirely. Prioritize Phase 21 or build a minimal shim for ESP32 only |
| Espressif QEMU not yet verified (22.5c)    | **High** | Emulator tests (23.5b/c) blocked until someone boots an ESP32-C3 binary in QEMU. Close 22.5c first |
| Arduino ESP32 core version changes         | High   | Pin arduino-esp32 version, test on updates   |
| WiFi ↔ zenoh socket bridging in C++        | Medium | Reuse pattern from Phase 22 BSP              |
| Library size too large for some boards     | Medium | Strip debug info, LTO, measure sizes         |
| ABI compatibility across compiler versions | Medium | Use C ABI only (no C++ mangling in library)  |
| `-icount 3` timing in QEMU vs wall-clock   | Medium | zenoh-pico timeouts may behave differently; tune or disable timeouts in test config |
| `espflash` CI dependency for flash images  | Low    | `espflash` is a cargo-installable tool, add to CI setup |
| Maintenance burden (rebuild per release)   | Medium | Automate with CI/CD                          |
| Arduino Library Manager acceptance         | Low    | Can distribute via GitHub releases initially |

## Comparison with micro_ros_arduino

| Feature                  | micro_ros_arduino                      | nros Arduino                          |
|--------------------------|----------------------------------------|-------------------------------------------|
| **User-facing API**      | Raw C (rcl/rclc functions)             | Raw C (nano_ros_* functions)              |
| C++ wrapper classes      | None                                   | None                                      |
| Transport setup          | `set_microros_wifi_transports()`       | `set_nanoros_wifi_transports()`           |
| Error handling           | `RCCHECK()` / `RCSOFTCHECK()` macros  | `NRCHECK()` / `NRSOFTCHECK()` macros     |
| Spin model               | `rclc_executor_spin_some()` in loop()  | `nano_ros_spin_once()` in loop()          |
| **Infrastructure**       |                                        |                                           |
| Agent required           | Yes (micro-ROS Agent on host)          | No (direct to zenohd)                     |
| Middleware               | Micro XRCE-DDS                         | zenoh-pico                                |
| Transport protocol       | Serial or UDP to Agent                 | TCP/UDP direct to zenohd                  |
| ROS 2 compatibility      | Via Agent (DDS bridge)                 | Via rmw_zenoh (native zenoh)              |
| **Library**              |                                        |                                           |
| Install size per board   | ~22 MB                                 | ~2 MB (estimate)                          |
| Boards supported         | 20+                                    | 3 initially (ESP32 family)                |
| Message types bundled    | ~200+ (58 packages)                    | Common types (std_msgs, geometry_msgs...) |
| Custom message types     | Rebuild library via Docker             | `cargo nano-ros generate` + rebuild       |
| `precompiled=true`       | Yes                                    | Yes                                       |
| **Performance**          |                                        |                                           |
| Latency                  | Higher (Agent relay)                   | Lower (direct zenoh)                      |
| Setup complexity         | High (Agent + ROS 2 install)           | Low (just zenohd binary)                  |

## Future Extensions

- Additional message packages beyond the initial bundle (action types, nav_msgs, tf2_msgs, etc.)
- Docker-based library rebuild for custom message types (following micro-ROS's `extras/library_generation/` pattern)
- Arduino Library Manager distribution
- PlatformIO integration
- Support for non-ESP32 boards (RP2040, STM32)
- Serial transport (`set_nanoros_serial_transports()`) for boards without WiFi/Ethernet
- Optional C++ convenience wrapper (thin classes over C API, for users who prefer OOP)
- Static memory configuration profiles (standard / low-memory / very-low-memory, following micro-ROS's `colcon.meta` approach)
- WebSocket transport for boards without raw TCP
