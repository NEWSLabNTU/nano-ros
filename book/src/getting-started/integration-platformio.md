# PlatformIO library

Single-node starter via the **PlatformIO library** path —
`platformio.ini` is the project manifest, `pio` is the build
orchestrator. Useful for Arduino-shaped or vendor-IDE-shaped
projects.

> **Prereqs.** PlatformIO Core ≥ 6.1 installed (`pip install
> platformio` or via the IDE plugin).

## Project layout

A PIO project is a CMake-free tree driven by `platformio.ini`:

```text
my_pio_app/
├── platformio.ini            # board, framework, lib_deps
├── lib/
│   └── nano-ros/             # local-path consumption (or via lib_deps URL)
└── src/
    └── main.cpp              # nros user code
```

`platformio.ini`:

```ini
[platformio]
default_envs = esp32-dev

[env:esp32-dev]
platform   = espressif32
framework  = arduino, espidf
board      = esp32dev
upload_speed = 921600
monitor_speed = 115200

lib_deps =
    # Local checkout during development:
    file://${PROJECT_DIR}/lib/nano-ros
    # Or git URL once available:
    # https://github.com/NEWSLabNTU/nano-ros.git#v<X.Y.Z>

build_flags =
    -D NANO_ROS_RMW=zenoh
    -D NANO_ROS_PLATFORM=esp32
```

The `nano-ros` library spec at
`integrations/platformio/library.json` declares the right include
paths, `build.unflags` for incompatible defaults, and the C/C++
source set. PIO's library manager pulls and builds it automatically.

## Configure

PlatformIO uses build flags as its native config surface; lift
runtime knobs into them via `-D` macros, or read from a side
`config.toml` if you prefer file-based config.

```ini
build_flags =
    -D NANO_ROS_RMW=zenoh
    -D NANO_ROS_DOMAIN_ID=0
    -D NANO_ROS_WIFI_SSID=\"your-ssid\"
    -D NANO_ROS_WIFI_PASSWORD=\"...\"
    -D NANO_ROS_LOCATOR=\"tcp/192.168.1.100:7447\"
```

## Build

```bash
cd my_pio_app
pio run
```

PIO's lib resolver picks up the nano-ros library spec, builds Rust
staticlibs (~3 min first time), and links the resulting `.a`
artifacts into your app.

## Run

```bash
pio run -t upload                 # flash + reset
pio device monitor                # serial console
# Expected:
#   Wifi connected
#   Published: 1
#   Published: 2

# Verify from stock ROS 2:
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
ros2 topic echo /chatter std_msgs/msg/Int32
```

**Readiness signal.** After `pio run -t upload` + `pio device
monitor`, expect `Wifi connected` then `Published: 1` within 10
seconds. If no `Published:` line:

1. Wrong build flag — `pio run -t envdump` should show the
   `NANO_ROS_*` macros from `platformio.ini` resolved.
2. Wi-Fi creds — same as ESP-IDF path; check the `-D` flags in
   `platformio.ini`.
3. Locator unreachable from the board's subnet.
4. See [Troubleshooting — First 10 Minutes](./troubleshooting-first-10-min.md).

## GitHub source

- PIO library spec:
  [`integrations/platformio/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/integrations/platformio)
- `library.json` manifest:
  [`integrations/platformio/library.json`](https://github.com/NEWSLabNTU/nano-ros/blob/main/integrations/platformio/library.json)

## Known sharp edges

- PlatformIO's `lib_deps` resolves transitively across env-level vs
  project-level scopes; if you see "library not found" errors, run
  `pio pkg list` and confirm nano-ros lands at the project level.
- Build flags must be quoted carefully when they contain string
  literals (note the escaped quotes on `NANO_ROS_LOCATOR` above).
- Cross-board reuse (ESP32 → STM32 → nRF) works as long as the
  underlying nano-ros target triple is supported; PIO's framework
  detection picks the right toolchain.

## Next

- esp-hal Rust path (no PIO): [ESP32 (esp-hal)](./esp32.md).
- ESP-IDF component path (C / C++, no PIO):
  [ESP32 (ESP-IDF component)](./integration-esp-idf.md).
- Arduino sketch path: see the Arduino chapter under
  [Choosing an RMW Backend](../user-guide/rmw-backends.md) (the
  nros-arduino wrapper rides on top of nros-c).
