# ESP32 (ESP-IDF component)

Single-node starter on ESP32-family chips via the **ESP-IDF
component path** — Espressif's native C / C++ build system. For the
bare-metal Rust (`esp-hal`) path, see [ESP32 (esp-hal)](./esp32.md).

> **Building nano-ros's own ESP32 examples from this repository?**
> The ESP-IDF C-port runs via `just esp_idf setup`, separate from
> the user-facing component documented here.

> **Prereqs.** ESP-IDF ≥ 5.1 installed and `idf.py` on `PATH`
> (`source $IDF_PATH/export.sh`).

## Project layout

ESP-IDF apps are CMake projects with `idf.py` as the orchestrator.
nano-ros plugs in as a component pulled by IDF's component manager
or by a local path during development.

```text
my_idf_app/
├── CMakeLists.txt                 # top-level: `project(my_app)`
├── sdkconfig                      # IDF Kconfig (generated)
├── main/
│   ├── CMakeLists.txt             # `idf_component_register(REQUIRES nano-ros …)`
│   ├── idf_component.yml          # declares nano-ros as a managed dependency
│   ├── app_main.c | app_main.cpp
│   └── config.toml
└── components/                    # (optional) local components override
```

The `idf_component.yml` is the dependency manifest:

```yaml
dependencies:
  nano-ros:
    # During development — local path to your nano-ros clone:
    path: ../../../nano-ros/integrations/esp-idf
    # Once published to the Espressif Component Registry:
    # version: "*"
```

The shell at `integrations/esp-idf/` wraps the nano-ros root CMake
into a standard IDF component, mapping IDF Kconfig knobs to
`NANO_ROS_*` cache vars.

## Configure

After `idf.py menuconfig`:

```
Component config → nano-ros
    [*] Enable nano-ros
        RMW backend       (zenoh)        zenoh | xrce | dds
        ROS 2 edition     (humble)
        Wi-Fi SSID        "your-ssid"
        Wi-Fi password    ********
```

Wi-Fi creds + zenoh locator can also live in a `config.toml`
alongside `app_main.c` if you prefer file-based config; the
component shell reads either source.

## Build

```bash
cd my_idf_app
idf.py set-target esp32c3        # or esp32s3, esp32, esp32c6
idf.py build
```

First build cross-compiles nano-ros's Rust staticlibs + IDF
components (~5 min). Re-builds finish in seconds.

## Run

```bash
# Flash + monitor:
idf.py -p /dev/ttyUSB0 flash monitor
# Expected serial output:
#   I (1234) nano-ros: Wi-Fi connected
#   I (1456) nano-ros: zenoh session opened
#   I (1567) nano-ros: Published: 1

# Verify from stock ROS 2 on the same network:
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
ros2 topic echo /chatter std_msgs/msg/Int32
```

QEMU ESP32 testing path: see the `just esp_idf` recipes — they
boot the IDF binary in `qemu-system-xtensa` via Espressif's
patched QEMU.

**Readiness signal.** After `idf.py flash monitor`, expect
`I (XXXX) nano-ros: Wi-Fi connected` followed by
`I (XXXX) nano-ros: Published: 1` within 10 seconds. If no
`Published:` line:

1. Wi-Fi creds — IDF Kconfig under `Component config → nano-ros`
   must carry SSID + password OR your `config.toml` must.
2. Wrong locator — confirm host running `zenohd` is on the same
   Wi-Fi subnet (or routable to it). NAT will block discovery.
3. `idf.py menuconfig` confirms `CONFIG_NROS_ENABLED=y`.
4. See [Troubleshooting — First 10 Minutes](./troubleshooting-first-10-min.md).

## GitHub source

- IDF component shell:
  [`integrations/esp-idf/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/integrations/esp-idf)
- Worked IDF example:
  [`integrations/esp-idf/examples/talker/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/integrations/esp-idf/examples/talker)
- Component manifest:
  [`integrations/esp-idf/idf_component.yml`](https://github.com/NEWSLabNTU/nano-ros/blob/main/integrations/esp-idf/idf_component.yml)

## Next

- Bare-metal `esp-hal` Rust path: [ESP32 (esp-hal)](./esp32.md).
- PlatformIO library path: [PlatformIO library](./integration-platformio.md).
- Multi-component IDF apps: nano-ros sits next to other Espressif
  components (network, storage, sensors) — IDF's component manager
  resolves them all.
