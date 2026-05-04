# micro-ROS UX gap analysis

Status: research notes  ·  Date: 2026-05-04
Sources: shallow clones in `external/` (`micro_ros_setup`, `rclc`, `micro_ros_zephyr_module`, `micro_ros_arduino`, `freertos_apps`, `micro-ROS-demos`)

## Executive summary

1. **One CLI for the whole pipeline.** micro-ROS gives users a single verb chain — `create_firmware_ws.sh` → `configure_firmware.sh` → `build_firmware.sh` → `flash_firmware.sh` — that abstracts RTOS, board, transport, and toolchain. nano-ros has rich `just` recipes but they are workspace-developer recipes, not "make-me-a-new-app" recipes. There is no `nano-ros new <board>` equivalent.
2. **Distribution as a precompiled artifact.** Users get micro-ROS as a static `.a` + `.h` (Arduino library zip, ESP-IDF component, Zephyr module, PlatformIO library, STM32CubeMX archive). nano-ros currently requires the user to clone the repo, run `just` recipes, and consume CMake `find_package(NanoRos)` from a build tree.
3. **Transport selection is a `-t` flag, not a Cargo feature matrix.** `configure_firmware.sh -t udp -i 192.168.1.100 -p 8888` swaps transport with no source-edit. nano-ros requires editing `config.toml`, `prj.conf`, or `Cargo.toml` features.
4. **Per-platform "extension" overlays are templated**, not standalone trees. `freertos_apps/microros_olimex_e407_extensions/` ships a complete CubeMX project + linker script + startup that the meta-build stitches into `firmware/`. nano-ros' "examples are standalone" rule (CLAUDE.md) is purer but means no template generator.
5. **Boilerplate parity gap.** rclc and nros-c hello-worlds are both ~50 lines, but rclc has uniform RTOS-agnostic transport setup (`rmw_uros_set_custom_transport(...)`) while nano-ros leaks platform specifics into user main (`zpico_zephyr_wait_network`, `APP_ZENOH_LOCATOR` macros from CMake, semihosting inits, etc.).

---

## 1. Project creation

### micro-ROS

The canonical 5-command flow (`micro_ros_setup/README.md`, `scripts/create_firmware_ws.sh:38`):

```bash
# 1. Once — inside a colcon workspace with micro_ros_setup built
ros2 run micro_ros_setup create_firmware_ws.sh freertos olimex-stm32-e407
# → fetches the right freertos_apps + STM32CubeMX extensions + uros packages
#   into ./firmware/  (driven by external/micro_ros_setup/config/freertos/...)

# 2. Pick app + transport
ros2 run micro_ros_setup configure_firmware.sh int32_publisher \
       -t udp -i 192.168.1.10 -p 8888

# 3. Build
ros2 run micro_ros_setup build_firmware.sh

# 4. Flash
ros2 run micro_ros_setup flash_firmware.sh
```

The `create_firmware_ws.sh` argument is consulted against
`external/micro_ros_setup/config/<rtos>/<platform>/` (`scripts/create_firmware_ws.sh:55`).
Each platform contributes its own `create.sh` / `configure.sh` / `build.sh` /
`flash.sh` — see `external/micro_ros_setup/config/freertos/olimex-stm32-e407/configure.sh`
and `flash.sh` for the full per-board recipe (OpenOCD probe auto-detection at
`flash.sh:8`).

The user picks an app from `freertos_apps/apps/` (e.g.
`external/freertos_apps/apps/int32_publisher/app.c`) — these are 50-line
sample C files that get compiled inside the prepared `firmware/` workspace.

**Adding a new app** = drop a folder into `freertos_apps/apps/` + `app.c` +
`app-colcon.meta` + `CMakeLists.txt` patterned on
`external/freertos_apps/apps/int32_publisher/`. No workspace-level
modification needed; `list_apps.sh` autodiscovers.

### nano-ros today

There is no project generator. New apps are created by *copying an example*
under `examples/<platform>/<lang>/<rmw>/<usecase>/` — CLAUDE.md explicitly
calls out "Each `examples/` dir is self-contained, copy-out template". The
talker layout (`examples/qemu-arm-freertos/c/zenoh/talker/`) requires the
user to also copy the sibling `cmake/freertos-support.cmake` referenced from
`CMakeLists.txt:5`.

Distribution-wise, the user needs the full nano-ros checkout because
`find_package(NanoRos CONFIG REQUIRED)` resolves to either
`build/install/` (after `just install-local`) or workspace-internal paths.
There is no precompiled `.a` shipped from CI.

**Gap:** no `nano-ros new <rtos> <board> <app>` scaffolder, no template
registry, no auto-discovery of user apps.

---

## 2. Build

### micro-ROS

Build is `ros2 run micro_ros_setup build_firmware.sh` (see
`external/micro_ros_setup/scripts/build_firmware.sh:1-60`). Per-platform
build delegates to `config/<rtos>/<platform>/build.sh` which is allowed to
do anything — for Olimex (`config/freertos/olimex-stm32-e407/build.sh:25`)
it `make libmicroros && make -j$(nproc)`. For Zephyr (`config/zephyr/generic/build.sh`)
it invokes `west build`.

The clever piece is `colcon.meta` overlay files: per-app meta files
(`external/freertos_apps/apps/int32_publisher/app-colcon.meta`) and global
meta files merged via `update_meta` (`config/utils.sh`) let the configure
step rewrite `RMW_UXRCE_DEFAULT_UDP_IP=...`, `UCLIENT_PROFILE_SERIAL=OFF`,
etc., without recompiling micro-ROS by hand.

### Zephyr alternative path

`micro_ros_zephyr_module` is a true Zephyr module — drop into a west
manifest and use directly:

```bash
west build -b disco_l475_iot1 -p   # micro_ros_zephyr_module/README.md:14
west build -t menuconfig            # → "Modules → micro-ROS support"
```

`prj.conf` is just `CONFIG_MICROROS=y` + main stack size +
`CONFIG_POSIX_API=y` (`micro_ros_zephyr_module/prj.conf:1-19`). The module
ships its own `microros_transports/` with stub transport functions that the
user's main calls (`micro_ros_zephyr_module/src/main.c:46`).

### nano-ros today

Multi-tier build: `just build`, `just build-examples`, `just build-test-fixtures`,
`just build-all` (CLAUDE.md). For users, the actual command is
`cmake -S examples/qemu-arm-freertos/c/zenoh/talker -B …/build && cmake
--build` (book/freertos.md:97). For Zephyr it is
`west build -b native_sim/native/64 nros/examples/zephyr/c/zenoh/talker`
(book/zephyr.md:101). Both are idiomatic — the gap is
*discovery*: the user must already know the example path and the toolchain
path. No top-level `nano-ros build my-app` wrapper.

**Strength to keep:** nano-ros' isolated example trees mean each project is
a real standalone CMake build (vs. micro-ROS' giant colcon workspace).
This is good for IDE integration, debugging, and license bundling. A
project-generator wrapper can preserve that.

---

## 3. Message generation

### micro-ROS

Two paths:

- **In-tree messages**: drop the `.msg` file into a colcon-discoverable
  package inside `firmware/mcu_ws/`. Pre-built micro-ROS includes a
  curated set under `built_packages` (see
  `external/micro_ros_arduino/built_packages` — 100+ messages including
  `geometry_msgs`, `sensor_msgs`, `nav_msgs`, `tf2_msgs`, `control_msgs`).
- **Custom Arduino**: rebuild the precompiled lib via Docker
  (`micro_ros_arduino/README.md:130`):
  ```bash
  docker run -it --rm -v $(pwd):/project \
       --env MICROROS_LIBRARY_FOLDER=extras \
       microros/micro_ros_static_library_builder:kilted
  ```
  Custom packages go in `extras/library_generation/extra_packages/`.

User-side, messages appear as plain C structs:
`std_msgs__msg__Int32` from `<std_msgs/msg/int32.h>`. Type support comes
from `ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, Int32)` (see
`external/freertos_apps/apps/int32_publisher/app.c:48`).

### nano-ros today

`cargo nano-ros generate-rust|generate-c|generate-cpp` reads
`package.xml` and emits typed bindings; CMake function
`nros_generate_interfaces(std_msgs "msg/Int32.msg" LANGUAGE C)`
(used at `examples/qemu-arm-freertos/c/zenoh/talker/CMakeLists.txt:7`)
runs codegen at configure time. No Docker, no workspace overlay.

The user surface in C is `std_msgs_msg_int32_get_type_support()` plus an
explicit `std_msgs_msg_int32_serialize(&msg, buf, sizeof buf, &len)` step
followed by `nros_publish_raw(&pub, buf, len)`
(`examples/qemu-arm-freertos/c/zenoh/talker/src/main.c:80-86`).

**Gap:** rclc users `rcl_publish(&publisher, &msg, NULL)` — the typed
struct is published directly, no manual buffer dance. nros-c forces
"serialize-then-publish-raw", which is a real ergonomic regression.

---

## 4. Transport / network configuration

### micro-ROS

Transport is configured by a CLI flag at firmware-config time:

```bash
ros2 run micro_ros_setup configure_firmware.sh int32_publisher \
     -t udp -i 192.168.1.10 -p 8888
```

The flag mutates `colcon.meta` entries
(`external/micro_ros_setup/config/freertos/olimex-stm32-e407/configure.sh:17-25`)
to flip `UCLIENT_PROFILE_UDP=ON` / `OFF`, set
`RMW_UXRCE_DEFAULT_UDP_IP/PORT`, etc. The next `build_firmware.sh`
recompiles with the new settings. **Source code does not change**.

For custom transports (USB CDC, BLE, semi-hosted serial, …), rclc exposes a
single C API at runtime:
```c
rmw_uros_set_custom_transport(true,
                              &my_params,
                              my_open, my_close,
                              my_write, my_read);
```
(see `external/micro_ros_zephyr_module/src/main.c:45`). User provides 4
function pointers; transport agnostic.

### nano-ros today

Transport choice is partly Cargo features (`rmw-zenoh` vs `rmw-xrce` vs
`rmw-dds`), partly board features (`ethernet` vs `wifi` vs `serial`,
CLAUDE.md "Board Transport Features"), partly per-app `config.toml`
(`examples/qemu-arm-freertos/c/zenoh/talker/config.toml:1-9`),
and partly Kconfig (`CONFIG_NROS_ZENOH_LOCATOR`,
`CONFIG_NROS_XRCE_AGENT_ADDR`). All of these are **edit-and-rebuild**.
There is no runtime transport plug-in vtable on the C side comparable to
`rmw_uros_set_custom_transport`.

**Gap:** "swap to serial-USB without touching source" needs a one-liner
(or no-edit CLI flag) on nano-ros too. Today the user must also swap
board crates between `nros-board-mps2-an385-freertos` (ethernet) and a
hypothetical serial board.

---

## 5. Flashing & running

### micro-ROS

`flash_firmware.sh` autodetects USB programmers (`flash.sh:7-23`) and
shells out to `openocd`/`west flash`/`idf.py flash`. Single command per
board.

The agent side is documented as a Docker one-liner in every README:
```bash
docker run -it --rm -v /dev:/dev --privileged --net=host \
     microros/micro-ros-agent:kilted serial --dev /dev/ttyUSB0 -v6
```
(`external/micro_ros_arduino/README.md:115`,
`external/micro_ros_zephyr_module/README.md:31`).

### nano-ros today

QEMU-based platforms have `just <plat> test`, but there is no `just flash`
or `nano-ros flash`. STM32F4 on real hardware uses
`packages/reference/stm32f4-porting/` for now. Real-hardware flashing on
NuttX, FreeRTOS, ThreadX is left to the user's own toolchain.

For agents, nano-ros docs name `zenohd` from `third-party/zenoh/` and
`MicroXRCEAgent` from setup. There is no published Docker image equivalent
to `microros/micro-ros-agent:<distro>`.

**Gap:** no first-class flashing recipe, no published Docker images for the
two RMW agents (Zenoh router, Micro-XRCE-DDS Agent).

---

## 6. Debugging

micro-ROS leaves debugging entirely to the platform (gdb via OpenOCD,
PlatformIO debug, west debug). Their docs are mute on this; users get
whatever their RTOS gives them.

nano-ros is *better* here: book/freertos.md has 200+ lines of LAN9118
debugging notes, register dumps, QEMU monitor tricks, and a Tonbandgeraet
trace integration (`NROS_TRACE=1`). This is a differentiator and should
stay.

---

## 7. RTOS abstraction

### micro-ROS

Identical user code across RTOSes — diff between `freertos_apps/apps/int32_publisher/app.c`
and `micro-ROS-demos/rclc/int32_publisher/main.c` is just the entry point
(`appMain(void *arg)` vs `main()`) and a `vTaskDelete(NULL)` at exit. Same
`rclc_support_init`, same `rclc_publisher_init_default`, same callback
shape. RTOS specifics live entirely in the *extension* templates supplied
by the build system, not in user code.

### nano-ros today

Compare:

- FreeRTOS (`examples/qemu-arm-freertos/c/zenoh/talker/src/main.c`):
  uses `app_main(void)`, no network-wait, relies on
  `APP_ZENOH_LOCATOR` macro injected via CMake.
- Zephyr (`examples/zephyr/c/zenoh/talker/src/main.c`):
  uses `int main(void)`, must call `zpico_zephyr_wait_network(...)` and
  reads `CONFIG_NROS_ZENOH_LOCATOR` from Kconfig.

Different entry, different network-ready handshake, different config
pickup (CMake `-D` macro vs Kconfig string). These are leaks of the
porting layer into user code.

**Gap:** unify on a single `nros_main(int argc, char **argv)` shim per
platform that handles network-readiness and locator pickup.

---

## 8. API ergonomics — side-by-side hello world

### rclc talker (FreeRTOS, `external/freertos_apps/apps/int32_publisher/app.c`)

- 50 source lines; uses 2 macros (`RCCHECK`, `RCSOFTCHECK`).
- Shape:

```c
allocator = rcl_get_default_allocator();
rclc_support_init(&support, 0, NULL, &allocator);
rclc_node_init_default(&node, "freertos_int32_publisher", "", &support);
rclc_publisher_init_default(&pub, &node,
        ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, Int32),
        "freertos_int32_publisher");
rclc_timer_init_default(&timer, &support, RCL_MS_TO_NS(1000), timer_cb);
rclc_executor_init(&executor, &support.context, 1, &allocator);
rclc_executor_add_timer(&executor, &timer);
while (1) { rclc_executor_spin_some(&executor, RCL_MS_TO_NS(100)); usleep(100000); }
```

- **Publication is a one-liner** inside the timer callback:
  `rcl_publish(&publisher, &msg, NULL);` — typed struct, no
  user-managed CDR buffer.

### nros-c talker (FreeRTOS, `examples/qemu-arm-freertos/c/zenoh/talker/src/main.c`)

- 98 source lines; uses no error macros (each call check is open-coded
  with branches that `printf` and `return`).
- Shape:

```c
nros_support_init(&app.support, APP_ZENOH_LOCATOR, APP_DOMAIN_ID);
nros_node_init(&app.node, &app.support, "c_talker", "/");
nros_publisher_init(&app.publisher, &app.node,
                    std_msgs_msg_int32_get_type_support(), "/chatter");
nros_executor_init(&app.executor, &app.support, 4);
for (;;) {
    for (int j = 0; j < 100; j++) nros_executor_spin_some(&app.executor, 10000000ULL);
    /* hand-written serialize → publish */
    uint8_t buf[64]; size_t n;
    std_msgs_msg_int32_serialize(&message, buf, sizeof buf, &n);
    nros_publish_raw(&app.publisher, buf, n);
    count++;
}
```

- **Publication is two steps** (serialize → publish_raw); no
  `nros_publish(&pub, &msg)` typed convenience.
- No timer concept exposed to user; user hand-rolls a 100×spin then
  publish loop. rclc has `rclc_timer_init_default` + `add_timer`.

| Metric                    | rclc talker  | nros-c talker |
|---------------------------|--------------|---------------|
| Source lines              | 50           | 98            |
| Init API calls            | 6            | 4             |
| Error-handling boilerplate | 2 macros (1-line) | 4 inline `if`-blocks (8 lines each) |
| Publish call sites        | 1 (typed)    | 3 (serialize + check + publish_raw) |
| User-visible CDR buffer   | no           | yes           |
| User-visible timer API    | yes (`rcl_timer_t`) | no |
| RTOS-specific symbols     | 1 (`vTaskDelete`) | 0 — but uses CMake `-D APP_ZENOH_LOCATOR` macro and `app_main` entry |

---

## 9. Project layout — user repo shape

### micro-ROS user repo

After `create_firmware_ws.sh`, the **user repo is the colcon workspace
itself** (`./firmware/` + `mcu_ws/`). Adding an app is one folder under
`freertos_apps/apps/<my_app>/` containing:

```
my_app/
├── app.c              (≤100 lines)
├── app-colcon.meta    (optional, RMW tunables)
└── CMakeLists.txt     (optional, often inherited from app-template)
```

For Arduino (`micro_ros_arduino/examples/micro-ros_publisher/micro-ros_publisher.ino`):
**a single `.ino` file** + zip-installed library. No CMake, no west, no
toolchain config in user-visible territory.

For Zephyr (`micro_ros_zephyr_module`): user adds the module to their
west manifest, drops `prj.conf` + `src/main.c` + `CMakeLists.txt` (3 files).

### nano-ros user repo (qemu-arm-freertos talker)

```
examples/qemu-arm-freertos/c/zenoh/talker/
├── CMakeLists.txt           (30 lines, must include sibling `freertos-support.cmake` from a path that escapes the example dir)
├── config.toml              (network + scheduling — TOML knobs)
└── src/main.c               (98 lines — see above)
plus implicit dependencies on:
  ../../../cmake/freertos-support.cmake   (relative escape)
  packages/boards/...                      (must be findable)
  third-party/freertos/                    (FREERTOS_DIR env)
```

The CLAUDE.md "examples are standalone" rule is aspirational — the
`include("${CMAKE_CURRENT_SOURCE_DIR}/../../../cmake/freertos-support.cmake")`
at line 5 of `CMakeLists.txt` is a hard escape from the example tree.

For Zephyr the layout is much cleaner (just `CMakeLists.txt`, `prj.conf`,
`boards/`, `src/main.c`) — close to micro-ROS Zephyr-module shape.

---

## 10. Distribution model

| Channel                | micro-ROS                                                          | nano-ros today                                  |
|------------------------|--------------------------------------------------------------------|-------------------------------------------------|
| Apt / rosdep           | `ros-<distro>-micro-ros-setup`                                     | none                                            |
| Source via colcon      | `ros2 run micro_ros_setup create_firmware_ws.sh ...` (one cmd)     | full repo clone + `just setup`                  |
| Precompiled `.a` + `.h`| `generate_lib` target → drop-in static lib                         | none                                            |
| Arduino library zip    | github releases/v2.0.8-kilted (`micro_ros_arduino` `library.properties`) | none                                          |
| ESP-IDF component      | `micro_ros_espidf_component`                                        | none (esp32-idf example uses workspace clone)   |
| Zephyr west module     | `micro_ros_zephyr_module` (drop in `west.yml`)                     | partial — `zephyr/` module dir but not consumable as external `west.yml` entry |
| PlatformIO library     | `micro_ros_platformio`                                             | none                                            |
| Docker agent           | `microros/micro-ros-agent:<distro>`                                | none — users build `zenohd` and `MicroXRCEAgent` themselves |
| STM32CubeMX            | `micro_ros_stm32cubemx_utils`                                       | none                                            |
| Renesas e² studio      | `micro_ros_renesas2estudio_component`                               | none                                            |

The micro-ROS list is 10+ distribution channels, all maintained as
separate repos under the `micro-ROS/` GitHub org. Each is small and
template-driven. nano-ros has one channel: "clone the monorepo".

---

## 11. Concrete UX improvement proposals (prioritized)

Each entry: **problem · micro-ROS approach · proposed nano-ros change · effort
(S/M/L) · risk**.

### P1 — `nano-ros` CLI scaffolder (`cargo nano-ros new`)

- **Problem.** No way to bootstrap a new app outside the monorepo. Users
  must copy an `examples/<…>/` tree manually.
- **micro-ROS.** `ros2 run micro_ros_setup create_firmware_ws.sh <rtos> <board>`
  fetches all needed pieces into `./firmware/`.
- **Proposal.** Extend `cargo-nano-ros` (already exists for codegen) with
  `cargo nano-ros new <rtos>-<board> --rmw <zenoh|xrce|dds> --lang <rust|c|cpp> <app-name>`.
  Templates live under `templates/` and mirror today's `examples/`. Output
  is a self-contained dir with `Cargo.toml` / `CMakeLists.txt` /
  `prj.conf` / `config.toml` / `src/main.{rs,c,cpp}` and a top-level
  `README.md` that names exactly which env vars need setting.
- **Effort.** M — ~1 week to template-ize 5 platforms × 3 langs × pub/sub.
- **Risk.** Low; existing examples are already templates.

### P2 — Typed publish in C/C++ (`nros_publish(&pub, &msg)`)

- **Problem.** `examples/qemu-arm-freertos/c/zenoh/talker/src/main.c:80-86`
  has the user serialize to a stack buffer then call `nros_publish_raw`.
  Doubles the LOC of every publish.
- **micro-ROS.** `rcl_publish(&publisher, &msg, NULL)` typed.
- **Proposal.** Generate per-message `nros_publish_<type>(&pub, &msg)`
  in the same codegen step that emits `<type>_serialize`. Internally
  serializes onto a per-publisher inline buffer (sized at compile time
  from the message bound) and calls `nros_publish_raw`. Provide an
  ergonomic typed macro `NROS_PUBLISH(pub, msg)` that picks the right
  function via `_Generic`.
- **Effort.** S — pure codegen change in `packages/codegen/rosidl-c/`.
- **Risk.** Low. Keeps the raw path for zero-copy users (Phase 99/103).

### P3 — Project generator templates as a separate repo `nano-ros-templates`

- **Problem.** `examples/` are simultaneously test fixtures and user
  templates. The "self-contained example" rule conflicts with the cmake
  include of `../../../cmake/freertos-support.cmake`
  (`examples/qemu-arm-freertos/c/zenoh/talker/CMakeLists.txt:5`).
- **micro-ROS.** Per-platform extension repos (`freertos_apps`,
  `micro_ros_zephyr_module`, …) shipped separately, versioned, releasable.
- **Proposal.** Spin up `nano-ros-templates/` as a sibling repo (or
  `templates/` subdir released as zip artifacts via CI). Each template is
  guaranteed to build *outside* the workspace (no relative escapes; `find_package(NanoRos)` and `find_package(NanoRosFreeRTOS)` only). The
  in-tree examples can `cmake -DNROS_TEMPLATE_DIR=... -P` re-render.
- **Effort.** M.
- **Risk.** Medium — requires nailing the relocatable `find_package` story;
  Phase 75 already did the install side, so most of the heavy lifting is
  done.

### P4 — Precompiled artifacts: Arduino zip, ESP-IDF component, west-installable Zephyr module, PlatformIO

- **Problem.** Single distribution channel ("clone the monorepo"). Users
  on Arduino IDE / PlatformIO have nowhere to start.
- **micro-ROS.** 10 distribution channels, all template-driven.
- **Proposal.** Ship per-target precompiled bundles from CI:
  - **Arduino**: `nano_ros_arduino-<rev>.zip` containing
    `src/<arch>/libnros.a` + `nros/*.h` + `library.properties`.
    Mirror `external/micro_ros_arduino/` shape.
  - **ESP-IDF component**: `idf_component.yml` registry entry.
  - **Zephyr west module**: publishable `west.yml` snippet pointing at a
    tagged release of nano-ros.
  - **PlatformIO library**: `library.json`.
- **Effort.** L — first artifact takes ~2 weeks (CI pipeline + cbindgen
  for headers + multi-target rust build matrix). Subsequent ones share
  infrastructure.
- **Risk.** Medium — Cargo features × RTOSes is combinatorial; need to
  pick a sensible default per target (likely `rmw-zenoh + serial`, with
  alternates as separate zips).

### P5 — Runtime transport vtable in nros-c (`nros_set_custom_transport`)

- **Problem.** Swapping serial-USB ↔ UDP today requires changing board
  crate, Cargo features, and `config.toml`. Users with custom HW (BLE
  bridges, USB-CDC, RS-485) have no extension hook in C.
- **micro-ROS.** `rmw_uros_set_custom_transport(framing, params, open,
  close, write, read)` — 4 function pointers, runtime-settable.
- **Proposal.** Expose `nros_set_custom_transport(struct
  nros_transport_ops *ops)` on the C side, backed by a feature-gated
  `zpico-platform-custom` (Zenoh) and an XRCE custom transport profile.
  Calls into the same trait that today's per-RTOS platform crates
  implement.
- **Effort.** M.
- **Risk.** Medium — needs a stable C trait shape; align with Phase 79
  (unified platform abstraction).

### P6 — Top-level `nano-ros` CLI for build/flash/agent

- **Problem.** `just` recipes are workspace-developer flow, not end-user
  flow. `just freertos build` assumes you are inside the monorepo.
- **micro-ROS.** `ros2 run micro_ros_setup build_firmware.sh` and
  `flash_firmware.sh` work from any colcon workspace.
- **Proposal.** Single binary `nano-ros` (Rust, in `packages/codegen/cargo-nano-ros`)
  with subcommands:
  - `nano-ros new` (P1)
  - `nano-ros build` — wraps `cmake`/`west`/`cargo build` per template
  - `nano-ros flash` — calls platform-specific flasher (OpenOCD config
    autodetect like `flash_firmware.sh:7-23`)
  - `nano-ros agent zenoh|xrce` — starts agent (or `docker run`s it)
- **Effort.** M — most work is wrapping existing `just` recipes.
- **Risk.** Low.

### P7 — Docker images for agents

- **Problem.** Users in book/zephyr.md:248 get told "Start `zenohd` /
  `MicroXRCEAgent`" — but they have to build them via `just zenohd setup`
  or apt-install separately.
- **micro-ROS.** `docker run microros/micro-ros-agent:kilted serial --dev
  /dev/ttyUSB0` — copy-paste works, no host install.
- **Proposal.** Publish two images to GHCR:
  `ghcr.io/newslabntu/nano-ros-zenoh-router:<tag>` and
  `ghcr.io/newslabntu/nano-ros-xrce-agent:<tag>`. Build from
  `third-party/zenoh/` and `third-party/Micro-XRCE-DDS-Agent/`. Mention
  the `docker run` command verbatim in every getting-started page.
- **Effort.** S — Dockerfiles + GH Actions matrix.
- **Risk.** Low.

### P8 — Single config-source (kill the CMake `-D` injection)

- **Problem.** FreeRTOS example uses *both* `config.toml` (parsed by
  `nano_ros_read_config()`) and `target_compile_definitions(... APP_*)`
  (`examples/qemu-arm-freertos/c/zenoh/talker/CMakeLists.txt:11-26`). The
  user's main.c then uses preprocessor macros (`APP_ZENOH_LOCATOR`,
  `APP_DOMAIN_ID`). Zephyr does this differently via Kconfig
  (`CONFIG_NROS_ZENOH_LOCATOR`).
- **micro-ROS.** Per-app `colcon.meta` overlay, mutated by configure CLI
  flag. User code reads `RMW_UXRCE_DEFAULT_UDP_IP` only inside transport
  init, never spread through main.
- **Proposal.** Auto-generate `nros_app_config.h` from `config.toml` (or
  Kconfig) at build time, expose a typed `nros_app_config_t` struct from
  `<nros/app_config.h>`. User writes `nros_support_init(&support,
  cfg.zenoh.locator, cfg.zenoh.domain_id)` instead of macros. Same
  generator on Zephyr reads Kconfig values into the same struct shape.
- **Effort.** S.
- **Risk.** Low.

### P9 — Unified `nros_app_main()` across RTOSes

- **Problem.** FreeRTOS example uses `void app_main(void)`, Zephyr uses
  `int main(void)` plus a manual `zpico_zephyr_wait_network()` call
  (`examples/zephyr/c/zenoh/talker/src/main.c:32`). Bare-metal will be
  different again. Users porting from one RTOS to another have to
  rewrite the entry shape and the network-ready dance.
- **micro-ROS.** Same `int main(int, char **)` signature across hosts;
  RTOS-specific entry shims live in the *extension* templates.
- **Proposal.** Define `int nros_app_main(int argc, char **argv)` as the
  *only* user-visible entry. Per-platform glue (`nros-platform-*`) calls
  it after network-wait, executor-init-context, board init, and
  RTOS-task-creation. Backward-compat: keep `app_main`/`main` as thin
  shims emitting deprecation warnings.
- **Effort.** S–M.
- **Risk.** Low.

### P10 — Curated message bundles (built-in ROS 2 type catalog)

- **Problem.** Users today bring their own `package.xml` and run
  `cargo nano-ros generate-{rust,c,cpp}` per package. There is no
  precompiled "common types" library.
- **micro-ROS.** `available_ros2_types`
  (`external/micro_ros_arduino/available_ros2_types`) is a curated list
  of >100 message/service/action types pre-included in the precompiled
  Arduino library. Users `#include <geometry_msgs/msg/twist.h>` and it
  Just Works.
- **Proposal.** Ship `nros-msgs-common` crate (and CMake target) that
  pre-generates `std_msgs`, `geometry_msgs`, `sensor_msgs`,
  `nav_msgs`, `tf2_msgs`, `lifecycle_msgs`, `action_msgs`,
  `example_interfaces` for both Rust and C/C++. Users add it to their
  CMake `target_link_libraries` instead of running codegen for each.
  Custom messages still flow through `cargo nano-ros generate-*`.
- **Effort.** M (mostly codegen plumbing, no new logic).
- **Risk.** Low.

### P11 (lower priority) — Timer + executor convenience

- **Problem.** nros-c users hand-roll
  `for (j=0;j<100;j++) nros_executor_spin_some(...)` then publish
  (`examples/qemu-arm-freertos/c/zenoh/talker/src/main.c:74-77`). rclc has
  first-class `rclc_timer_init_default` + `rclc_executor_add_timer`.
- **Proposal.** Already exists in `nros/timer.h` (per book/c-api.md);
  examples should be rewritten to use it.
- **Effort.** S.
- **Risk.** Trivial.

### P12 (lower priority) — Default error-check macros

- **Problem.** Every nros-c hello-world repeats 4-line `if (ret != …) {
  printf; cleanup; return; }` blocks. rclc users copy `RCCHECK`/`RCSOFTCHECK`
  but at least they're 1-line macros.
- **Proposal.** Ship `<nros/check.h>` with `NROS_CHECK(call)` and
  `NROS_SOFTCHECK(call)` macros (one-liner, log-and-bail). Use them in
  every example.
- **Effort.** S (header + example sweep).
- **Risk.** None.

---

## 12. What nano-ros already does better — keep these

- **Rust-core, Cargo-native.** Memory safety + true `no_std`. Don't
  trade this for a colcon-meta workspace.
- **Compile-time bounded executors.** `NROS_EXECUTOR_MAX_CBS` / arena
  size at compile time vs micro-ROS' `rclc_executor_init(executor, ctx,
  num_handles, &alloc)` runtime sizing. Bound is enforceable; no
  surprise heap allocation.
- **CMake `find_package(NanoRos)` discipline** (CLAUDE.md "CMake Path
  Convention"). micro-ROS' meta-build system relies on relative paths
  and colcon overlays that are notoriously hard to consume out-of-tree.
- **Verification (Verus/Kani).** micro-ROS has none.
- **Per-platform tracing + LAN9118 debugging guides** (book/freertos.md).
- **Strict ghost-typed `Result<…, RclrsError>` Rust API.** Nicer than
  `rcl_ret_t` integers.

The proposals above are about the *meta layer* (project gen, CLI,
distribution, transport plug-in) — none of them require rewriting the
Rust core.

---

## Appendix A — Files cited

| Path                                                                                  | What                              |
|---------------------------------------------------------------------------------------|-----------------------------------|
| `external/micro_ros_setup/scripts/create_firmware_ws.sh:38-66`                        | RTOS/board dispatch               |
| `external/micro_ros_setup/scripts/configure_firmware.sh:50-78`                        | `-t/-i/-p/-d` transport CLI       |
| `external/micro_ros_setup/scripts/build_firmware.sh:1-60`                             | per-platform build delegation     |
| `external/micro_ros_setup/scripts/flash_firmware.sh:1-35`                             | flash dispatch                    |
| `external/micro_ros_setup/config/freertos/olimex-stm32-e407/configure.sh:17-49`       | colcon.meta mutation              |
| `external/micro_ros_setup/config/freertos/olimex-stm32-e407/flash.sh:7-23`            | OpenOCD probe autodetect          |
| `external/micro_ros_zephyr_module/CMakeLists.txt`                                     | 9-line Zephyr CMake               |
| `external/micro_ros_zephyr_module/prj.conf`                                           | 19-line Kconfig                   |
| `external/micro_ros_zephyr_module/src/main.c:42-90`                                   | rclc Zephyr hello-world           |
| `external/freertos_apps/apps/int32_publisher/app.c`                                   | rclc FreeRTOS hello-world         |
| `external/rclc/rclc/include/rclc/executor.h:120-957`                                  | rclc_executor API surface         |
| `external/rclc/rclc_examples/src/example_executor.c:60-130`                           | full pub/sub/timer example        |
| `external/micro_ros_arduino/library.properties`                                       | Arduino-IDE distribution metadata |
| `external/micro_ros_arduino/examples/micro-ros_publisher/micro-ros_publisher.ino`     | Arduino .ino hello-world          |
| `external/micro_ros_arduino/README.md:130`                                            | Docker-based custom rebuild       |
| `examples/qemu-arm-freertos/c/zenoh/talker/CMakeLists.txt:5,7,11-26`                  | nros-c CMake                      |
| `examples/qemu-arm-freertos/c/zenoh/talker/src/main.c:30-97`                          | nros-c FreeRTOS hello-world       |
| `examples/qemu-arm-freertos/c/zenoh/talker/config.toml:1-22`                          | TOML knobs                        |
| `examples/zephyr/c/zenoh/talker/src/main.c:25-95`                                     | nros-c Zephyr hello-world         |
| `examples/zephyr/c/zenoh/talker/prj.conf:54-62`                                       | nros Zephyr Kconfig               |
| `book/src/getting-started/freertos.md:51-103`                                         | nano-ros FreeRTOS user docs       |
| `book/src/getting-started/zephyr.md:55-170`                                           | nano-ros Zephyr user docs         |
| `book/src/reference/c-api.md`                                                         | nros-c surface index              |
| `justfile:38-54`                                                                      | top-level just modules            |
