# Phase 23: Precompiled C Library for Arduino IDE

**Goal**: Provide a precompiled nano-ros Arduino library that enables Arduino IDE users to publish/subscribe ROS 2 topics with a familiar Arduino-style API — no Rust toolchain required.

**Status**: Not Started
**Priority**: Medium
**Depends on**: Phase 22 (ESP32 support), Phase 11 (C API)

## Overview

Arduino is the most widely used embedded development platform, especially in education and hobbyist communities. By providing a precompiled Arduino library, nano-ros can reach users who would never install a Rust toolchain.

This follows the same approach as micro-ROS Arduino (`micro_ros_arduino`): precompile `libnanoros.a` for each supported board, wrap it with an Arduino-compatible `.h` header, and distribute as an Arduino Library.

### Key Advantage Over micro-ROS Arduino

| Aspect | micro-ROS Arduino | nano-ros Arduino |
|--------|-------------------|------------------|
| Agent required | Yes (micro-ROS Agent on host) | **No** (direct zenoh) |
| Network | Serial to Agent (typically) | WiFi direct to zenohd |
| Setup complexity | High (Agent + ROS 2 install) | **Low** (just zenohd) |
| Latency | Higher (Agent relay) | **Lower** (direct) |
| Board support | Many (precompiled per board) | ESP32 family initially |

### Target Boards (Initial)

| Board | Chip | Arduino Core | Priority |
|-------|------|-------------|----------|
| Arduino Nano ESP32 | ESP32-S3 | arduino-esp32 | High |
| ESP32-C3-DevKitC | ESP32-C3 | arduino-esp32 | High |
| ESP32-DevKitC | ESP32 | arduino-esp32 | Medium |
| Arduino Nano RP2040 Connect | RP2040 + Nina W102 | arduino-mbed | Future |

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                    Arduino IDE Sketch                            │
│                                                                  │
│  #include <NanoRos.h>                                           │
│  NanoRosNode node;                                              │
│  NanoRosPublisher pub;                                          │
│  node.begin("MyWiFi", "pass", "tcp/192.168.1.1:7447");         │
│  pub = node.advertise("/chatter");                              │
│  pub.publish(msg, len);                                         │
│  node.spinOnce();                                               │
└─────────────────────────────┬───────────────────────────────────┘
                              │
┌─────────────────────────────▼───────────────────────────────────┐
│                     NanoRos.h / NanoRos.cpp                      │
│              (Arduino-style C++ wrapper)                        │
│                                                                  │
│  - NanoRosNode, NanoRosPublisher, NanoRosSubscriber classes     │
│  - WiFi initialization via Arduino WiFi.h                       │
│  - Translates Arduino API → C API calls                        │
└─────────────────────────────┬───────────────────────────────────┘
                              │
┌─────────────────────────────▼───────────────────────────────────┐
│                   libnanoros.a (precompiled)                     │
│                                                                  │
│  - nano-ros-c (C API)                                           │
│  - zenoh-pico-shim-sys (C shim + zenoh-pico)                   │
│  - Platform layer (smoltcp or lwIP)                             │
└─────────────────────────────────────────────────────────────────┘
```

### Build Pipeline

```
┌─────────────────────────────────────────────────────────────────┐
│                    Build Server / CI                             │
│                                                                  │
│  For each target board:                                         │
│  1. Cross-compile zenoh-pico → libzenohpico.a                  │
│  2. Cross-compile nano-ros C shim → libnanoros_shim.a           │
│  3. Bundle: libnanoros.a = libzenohpico.a + libnanoros_shim.a   │
│  4. Generate header: nano_ros.h (cbindgen from C API)          │
│  5. Package into Arduino library structure                      │
│                                                                  │
│  Output per board:                                              │
│    src/<board>/libnanoros.a                                      │
│    src/nano_ros.h                                               │
└─────────────────────────────────────────────────────────────────┘
```

## Target API

### Arduino Sketch (Talker)

```cpp
#include <WiFi.h>
#include <NanoRos.h>

const char* ssid = "MyNetwork";
const char* password = "password123";
const char* zenoh_locator = "tcp/192.168.1.1:7447";

NanoRosNode node;
NanoRosPublisher pub;
int count = 0;

void setup() {
    Serial.begin(115200);

    WiFi.begin(ssid, password);
    while (WiFi.status() != WL_CONNECTED) {
        delay(500);
        Serial.print(".");
    }
    Serial.println("WiFi connected");

    node.begin(zenoh_locator);
    pub = node.advertise("/chatter");
}

void loop() {
    int32_t msg = count++;
    pub.publish((uint8_t*)&msg, sizeof(msg));
    Serial.printf("Published: %d\n", msg);

    node.spinOnce();
    delay(1000);
}
```

### Arduino Sketch (Listener)

```cpp
#include <WiFi.h>
#include <NanoRos.h>

NanoRosNode node;
NanoRosSubscriber sub;

void messageCallback(const uint8_t* data, size_t len, void* context) {
    if (len >= 4) {
        int32_t value;
        memcpy(&value, data, sizeof(value));
        Serial.printf("Received: %d\n", value);
    }
}

void setup() {
    Serial.begin(115200);
    WiFi.begin("MyNetwork", "password123");
    while (WiFi.status() != WL_CONNECTED) delay(500);

    node.begin("tcp/192.168.1.1:7447");
    sub = node.subscribe("/chatter", messageCallback, NULL);
}

void loop() {
    node.spinOnce();
    delay(10);
}
```

## Implementation Plan

### 23.1: Arduino Library Structure

**Status**: Not Started

**Tasks**:
1. [ ] Create Arduino library directory:
   ```
   arduino/nano-ros/
   ├── library.properties        # Arduino library metadata
   ├── keywords.txt              # Syntax highlighting
   ├── src/
   │   ├── NanoRos.h             # Main include (C++ wrapper)
   │   ├── NanoRos.cpp           # C++ wrapper implementation
   │   ├── nano_ros_c.h          # C API header (from cbindgen)
   │   ├── esp32s3/
   │   │   └── libnanoros.a      # Precompiled for ESP32-S3
   │   ├── esp32c3/
   │   │   └── libnanoros.a      # Precompiled for ESP32-C3
   │   └── esp32/
   │       └── libnanoros.a      # Precompiled for ESP32
   ├── examples/
   │   ├── Talker/
   │   │   └── Talker.ino
   │   ├── Listener/
   │   │   └── Listener.ino
   │   └── ServiceClient/
   │       └── ServiceClient.ino
   └── README.md
   ```
2. [ ] Create `library.properties` with metadata
3. [ ] Create `keywords.txt` for IDE syntax highlighting
4. [ ] Design C++ wrapper classes (`NanoRosNode`, `NanoRosPublisher`, `NanoRosSubscriber`)

**Acceptance Criteria**:
- [ ] Library structure follows Arduino Library Specification 2.2
- [ ] C++ wrapper API is idiomatic Arduino style

### 23.2: Precompilation Build System

**Status**: Not Started

**Tasks**:
1. [ ] Create `scripts/arduino/build-precompiled.sh`
   - Cross-compile zenoh-pico for each target architecture
   - Cross-compile zenoh_shim.c for each target
   - Bundle into single `libnanoros.a` per target
2. [ ] Add `just build-arduino-libs` recipe
3. [ ] Generate `nano_ros_c.h` header from C API (cbindgen or manual)
4. [ ] Test linking precompiled library with Arduino ESP32 core
5. [ ] Create CI workflow for automated builds on release

**Acceptance Criteria**:
- [ ] `libnanoros.a` built for ESP32-S3, ESP32-C3, ESP32
- [ ] Header matches library ABI
- [ ] Arduino IDE can compile sketches linking the library

### 23.3: C++ Arduino Wrapper

**Status**: Not Started

**Tasks**:
1. [ ] Implement `NanoRosNode` class
   ```cpp
   class NanoRosNode {
   public:
       bool begin(const char* zenoh_locator);
       NanoRosPublisher advertise(const char* topic);
       NanoRosSubscriber subscribe(const char* topic, callback_t cb, void* ctx);
       void spinOnce();
       void end();
   };
   ```
2. [ ] Implement `NanoRosPublisher` class
   ```cpp
   class NanoRosPublisher {
   public:
       bool publish(const uint8_t* data, size_t len);
   };
   ```
3. [ ] Implement `NanoRosSubscriber` class
4. [ ] Handle WiFi ↔ zenoh-pico socket bridging
5. [ ] Error handling with Arduino Serial output

**Acceptance Criteria**:
- [ ] Wrapper compiles with Arduino ESP32 core
- [ ] API is familiar to Arduino users
- [ ] Error messages printed to Serial

### 23.4: Example Sketches

**Status**: Not Started

**Tasks**:
1. [ ] Create `Talker.ino` — publish Int32 every second
2. [ ] Create `Listener.ino` — subscribe and print received messages
3. [ ] Create `ServiceClient.ino` — call AddTwoInts service
4. [ ] Each example includes WiFi setup boilerplate
5. [ ] Test all examples on Arduino Nano ESP32

**Acceptance Criteria**:
- [ ] Examples compile in Arduino IDE without modification
- [ ] Examples work out-of-the-box on Arduino Nano ESP32
- [ ] Each example includes clear comments

### 23.5: Testing and Documentation

**Status**: Not Started

**Tasks**:
1. [ ] Test Arduino Talker ↔ native Listener (via zenohd)
2. [ ] Test native Talker ↔ Arduino Listener (via zenohd)
3. [ ] Test Arduino ↔ ROS 2 interop (via rmw_zenoh)
4. [ ] Create `README.md` with:
   - Installation instructions (Arduino Library Manager or manual)
   - Quick start guide
   - WiFi configuration
   - Troubleshooting
5. [ ] Create `docs/arduino-setup.md` with detailed guide
6. [ ] Consider Arduino Library Manager submission (future)

**Acceptance Criteria**:
- [ ] Bidirectional communication works
- [ ] README enables new users to get running in <15 minutes
- [ ] Tested on at least 2 ESP32 boards

## Dependencies

```
Phase 11 (C API) ──────────────────────────┐
                                            │
Phase 22 (ESP32 support) ──────────────────┤
                                            │
                                            ▼
23.1 (Library structure) ──────────────────┤
                                            │
23.2 (Build system) ───────────────────────┤
                                            │
                                            ▼
23.3 (C++ wrapper) ────────────────────────┤
                                            │
                                            ▼
23.4 (Examples) ───────────────────────────┤
                                            │
                                            ▼
23.5 (Testing + docs) ────────────────────┘
```

Phase 22 (ESP32 native Rust) is a prerequisite because:
- It validates zenoh-pico cross-compilation for ESP32 targets
- It establishes the smoltcp ↔ WiFi bridge pattern
- The precompiled library reuses the same zenoh-pico + shim build

Phase 11 (C API) provides the `nano_ros_c.h` functions that the Arduino wrapper calls.

## Risks and Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Arduino ESP32 core version changes | High | Pin arduino-esp32 version, test on updates |
| WiFi ↔ zenoh socket bridging in C++ | Medium | Reuse pattern from Phase 22 BSP |
| Library size too large for some boards | Medium | Strip debug info, LTO, measure sizes |
| ABI compatibility across compiler versions | Medium | Use C ABI only (no C++ mangling in library) |
| Maintenance burden (rebuild per release) | Medium | Automate with CI/CD |
| Arduino Library Manager acceptance | Low | Can distribute via GitHub releases initially |

## Comparison with micro_ros_arduino

| Feature | micro_ros_arduino | nano-ros Arduino |
|---------|-------------------|------------------|
| Install size | ~10 MB per board | ~2 MB per board (estimate) |
| Agent required | Yes | No |
| Transport | Serial (default) | WiFi (direct) |
| Boards supported | 20+ | 3 initially (ESP32 family) |
| ROS 2 messages | Generated per project | Raw bytes (CDR optional) |
| Rebuild required | On message type change | Never (raw bytes) |
| Latency | Higher (Agent relay) | Lower (direct zenoh) |
| Complexity | High (Agent + ROS 2) | Low (just zenohd) |

## Future Extensions

- Typed message support (code-generated `.h` files per message type)
- Arduino Library Manager distribution
- PlatformIO integration
- Support for non-ESP32 boards (RP2040, STM32)
- ArduinoJson integration for human-readable message formatting
- WebSocket transport for boards without raw TCP
