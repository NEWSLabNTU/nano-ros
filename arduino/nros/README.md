# nros — Arduino library

**Status: skeleton (Phase 23.1). Not yet shippable — the precompiled
`libnanoros.a` slots under `src/<arch>/` are empty until Phase 21.6
and Phase 23.2 land. See
`docs/roadmap/phase-23-arduino-precompiled.md` for the live status.**

This directory will ship as the `nros` Arduino library: an ESP32 user
opens Arduino IDE, installs `nros` from Library Manager (or imports
the zip from a GitHub Release), and publishes/subscribes to ROS 2
topics with a few function calls. No agent, no `colcon`, no Rust
toolchain on the developer's machine.

## Quick start (target user experience)

```cpp
#include <nros_arduino.h>
#include <nros/init.h>
#include <nros/node.h>
#include <nros/publisher.h>
#include <std_msgs/msg/int32.h>

nros_context_t ctx;
nros_node_t node;
nros_publisher_t pub;
int count = 0;

void setup() {
    Serial.begin(115200);
    set_nanoros_wifi_transports("MyNetwork", "password",
                                 "tcp/192.168.1.1:7447");
    NRCHECK(nros_init(&ctx));
    NRCHECK(nros_node_create(&node, &ctx, "talker"));
    NRCHECK(nros_publisher_create(&pub, &node, "/chatter",
        NANO_ROS_MSG_TYPE_SUPPORT(std_msgs, msg, Int32)));
}

void loop() {
    std_msgs__msg__Int32 msg = { .data = count++ };
    NRSOFTCHECK(nros_publish(&pub, &msg, sizeof(msg)));
    nros_spin_once(&ctx, 100);
    delay(1000);
}
```

A `ros2 topic echo /chatter` on the host (with `rmw_zenoh` selected)
sees every published message, no agent process required.

## Why nros over micro-ROS for Arduino

- **No micro-ROS Agent.** nros sketches connect directly to `zenohd`,
  which is a single static binary you launch beside `ros2`. Setup
  collapses to "start zenohd, flash the sketch".
- **Smaller install.** Each `libnanoros.a` is built without the
  Micro XRCE-DDS layer — measure once 23.2 lands, target ~2 MB per
  chip.
- **ros2 interop via `rmw_zenoh`.** The sketch's topics show up in
  `ros2 topic list` natively, with the same wire format upstream
  rmw_zenoh uses.
- **Same Arduino-friendly C API shape.** `set_nanoros_wifi_transports`
  / `NRCHECK` / `nros_spin_once` mirror micro-ROS's
  `set_microros_wifi_transports` / `RCCHECK` / `rclc_executor_spin_some`
  on purpose; migration is one find-and-replace.

## What's in here

| Path                                  | Purpose                                                                                |
|---------------------------------------|----------------------------------------------------------------------------------------|
| `library.properties`                  | Arduino IDE library manifest. `precompiled=true` + `architectures=esp32`.              |
| `keywords.txt`                        | Arduino IDE syntax highlighting for `nros_*` + macros.                                 |
| `src/nros_arduino.{h,cpp}`            | Arduino-specific glue (~70 lines): `set_nanoros_wifi_transports`, `NRCHECK`, ping.     |
| `src/nros/`                           | C API headers, copied from `packages/core/nros-c/include/`.                            |
| `src/std_msgs/`, `src/geometry_msgs/`, `src/sensor_msgs/` | Pre-generated ROS message C headers (`cargo nano-ros generate-c`).                     |
| `src/esp32/libnanoros.a`              | Precompiled archive for ESP32 (Xtensa LX6). Empty until Phase 23.2.                    |
| `src/esp32s3/libnanoros.a`            | Precompiled archive for ESP32-S3 (Xtensa LX7).                                         |
| `src/esp32c3/libnanoros.a`            | Precompiled archive for ESP32-C3 (RISC-V).                                             |
| `examples/Talker/Talker.ino`          | Minimal `std_msgs/Int32` publisher.                                                    |
| `examples/Listener/Listener.ino`      | Matching subscriber.                                                                   |

## Building this library locally (contributor flow)

Eventually, `just package-arduino` (Phase 23.2.4) will:

1. Run `just esp_idf setup` if ESP-IDF is missing (`tier=extended`).
2. Cross-build `libnros_c.a` + `libzpico.a` for each target chip via
   the Phase 139 ESP-IDF integration shell.
3. Bundle each pair into `arduino/nros/src/<arch>/libnanoros.a` with
   `ar crsT`.
4. Re-generate the bundled message headers via
   `cargo nano-ros generate-c`.
5. Zip the directory into `nano-ros-arduino-v<version>.zip` for
   Release distribution.

Until then, the precompiled slots are empty and the sketch examples
will not link.

## Status & roadmap

This is Phase 23 — see [`docs/roadmap/phase-23-arduino-precompiled.md`](../../docs/roadmap/phase-23-arduino-precompiled.md)
for the full plan. Phase 21.6–21.10 (reopened in 2026-05) must land
first so `nros-c` can cross-compile against ESP-IDF.
