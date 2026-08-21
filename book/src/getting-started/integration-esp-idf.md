# ESP32 (ESP-IDF component)

Single-node starter on ESP32-family chips via the **ESP-IDF
component path** — Espressif's native C / C++ build system. For the
bare-metal Rust (`esp-hal`) path, see [ESP32 (esp-hal)](./esp32.md).

> **Prereqs.** Two independent toolchains.
>
> 1. **ESP-IDF itself** — ≥ 5.1, installed through Espressif's own
>    installer so `idf.py` is on `PATH` (`source $IDF_PATH/export.sh`).
>    `nros setup` does **not** replace this; the IDF toolchain comes
>    from `idf.py install` / Espressif's tooling.
> 2. **The nano-ros side** — the RMW host daemon (and any nano-ros
>    host tools you use for testing) come from the `nros` CLI:
>
>    ```bash
>    ./scripts/bootstrap.sh      # builds packages/cli/target/release/nros (Phase 218)
>    source ./activate.sh        # OR: direnv allow / source ./activate.fish
>    nros setup qemu-esp32-baremetal --rmw zenoh     # lands the toolchain
>                                     # (and for xrce the Micro-XRCE-DDS
>                                     # agent) in ${NROS_HOME:-~/.nros}/sdk,
>                                     # AND clones the transport submodules.
>                                     # The zenoh router is NOT installed —
>                                     # it comes from ROS 2
>                                     # (ros2 run rmw_zenoh_cpp rmw_zenohd)
>                                     # (zenoh-pico + mbedtls for zenoh)
>                                     # into the nano-ros checkout
>                                     # so the IDF build can compile
>                                     # them in-tree.
>    ```

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
│   └── app_main.c | app_main.cpp
└── components/                    # (optional) local components override
```

The `idf_component.yml` is the dependency manifest:

```yaml
dependencies:
  nano-ros:
    # During development — local path to your nano-ros clone:
    path: ../../../nano-ros/integrations/nano-ros
    # Once published to the Espressif Component Registry:
    # version: "*"
```

The shell at `integrations/nano-ros/` wraps the nano-ros root CMake
into a standard IDF component, mapping IDF Kconfig knobs to
`NANO_ROS_*` cache vars.

## Configure

After `idf.py menuconfig`:

```
Component config → nano-ros
    Bringup package name (demo_bringup)
    ROS domain ID        (0)
    RMW backend          (xrce)         xrce | zenoh
    ROS 2 edition        (humble)       humble | iron | jazzy
```

Four knobs. The default RMW is **XRCE** (CycloneDDS is intentionally
absent on ESP — the Kconfig says so). `NROS_BRINGUP_NAME` drives the
configure-time `nros codegen-system` step, so it must name a bringup
package that exists in your workspace — a first build with the default
`demo_bringup` fails if you have none.
**Wi-Fi credentials + zenoh locator are NOT in this Kconfig** —
provide them via your app's own `Kconfig.projbuild` (Espressif's
standard pattern) or via environment variables, then pass them to
`nros::init(locator, domain_id)` at startup.

## Build

> **Honesty note:** a complete reference app (Wi-Fi + locator wiring on
> top of the component) is not in-tree yet — the component shell builds
> and boots, but you author the app glue. For a worked end-to-end flow
> today, start with the [QEMU esp-hal chapter](./esp32.md) and come back
> here to embed into your IDF project.

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
#   I (2567) nano-ros: Publishing: 'Hello World: 1'

# Verify from stock ROS 2 on the same network:
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
# Talker publishes best-effort; stock `ros2 topic echo` defaults to
# RELIABLE, so the QoS-mismatched echo silently delivers nothing.
# Force best-effort to receive:
ros2 topic echo /chatter std_msgs/msg/String --qos-reliability best_effort
```

**Contributors (in-tree checkout):** for the QEMU ESP32 testing path
see `just esp_idf test-c-port` — it boots the `tests/esp-idf-smoke`
C-port project on `qemu-system-riscv32` (esp32c3 default) via
Espressif's patched QEMU.

**Readiness signal.** After `idf.py flash monitor`, expect
`I (XXXX) nano-ros: Wi-Fi connected` followed by
`I (XXXX) nano-ros: Publishing: 'Hello World: 1'` within 10 seconds
— Rust + C + C++ talkers all start the count at 1, matching the
official ROS 2 demo talker. If no `Publishing:` line:

1. Wi-Fi creds — your app's own Kconfig (nano-ros's submenu does not
   carry them; see Configure above).
2. Wrong locator — confirm the host running the router/agent is on the
   same Wi-Fi subnet (or routable to it). NAT will block discovery.
3. `idf.py menuconfig` shows the `Component config → nano-ros` submenu
   (the component is wired) and `CONFIG_NROS_RMW` is set to a backend
   name (`xrce`/`zenoh`). There is no separate
   `CONFIG_NROS_ENABLED` toggle on ESP-IDF; the component's presence
   in `main/idf_component.yml` is the on-switch.
4. See [Troubleshooting — First 10 Minutes](./troubleshooting-first-10-min.md).

## GitHub source

- IDF component shell:
  [`integrations/nano-ros/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/integrations/nano-ros)
- Component manifest:
  [`integrations/nano-ros/idf_component.yml`](https://github.com/NEWSLabNTU/nano-ros/blob/main/integrations/nano-ros/idf_component.yml)
- Kconfig surface:
  [`integrations/nano-ros/Kconfig.projbuild`](https://github.com/NEWSLabNTU/nano-ros/blob/main/integrations/nano-ros/Kconfig.projbuild)

A complete reference app showing Wi-Fi + zenoh wiring on top of the
component is not in-tree yet; the bare-metal
[`examples/qemu-esp32-baremetal/rust/talker/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/qemu-esp32-baremetal/rust/talker)
is the closest worked example.

## Next

- Bare-metal `esp-hal` Rust path: [ESP32 (esp-hal)](./esp32.md).
- Multi-component IDF apps: nano-ros sits next to other Espressif
  components (network, storage, sensors) — IDF's component manager
  resolves them all.
