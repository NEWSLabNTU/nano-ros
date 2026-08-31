# How integration works

You already have an RTOS project that builds and runs — an STM32Cube or
NXP MCUXpresso tree, a Pico SDK app, an ESP-IDF project, a vendored
Zephyr or NuttX workspace. This section is about adding nano-ros to
**that** project, without changing who is in charge.

## The principle

**nano-ros is a library your project imports. Your RTOS keeps its own
build tool.** You keep building with `west`, `make`, `idf.py`, `cmake`,
or your IDE, exactly as before. nano-ros never compiles your kernel,
never owns your linker script, and never replaces your build system.

What nano-ros contributes to your build is small and fixed:

- a **Rust static library** (the client runtime, behind a stable C ABI),
- **generated code** for your messages and system configuration,
- a thin **C shim** that your build compiles, so it sees *your* config
  headers (`FreeRTOSConfig.h`, `lwipopts.h`, `sdkconfig`, …).

The last point is why the shim is source, not a prebuilt: config-header
mismatches between your kernel and a prebuilt binary are silent ABI
breaks. Your compiler, your headers, your flags.

## One shape per host build

Each supported host build system gets a small "shell" — build glue plus
the host's own package manifest — that makes nano-ros look native to it.
Pick your row:

| Your build | How nano-ros plugs in | Shell in the repo | Start here |
| --- | --- | --- | --- |
| Zephyr (`west`) | west module | `zephyr/` (`module.yml`, `Kconfig`, `CMakeLists.txt`) | [Zephyr (west module)](./integration-zephyr.md) |
| NuttX (`make` + Kconfig) | `apps/external/` app | `integrations/nuttx/` (`Make.defs`, `Kconfig`, `Makefile`) | [NuttX (apps/external)](./integration-nuttx.md) |
| ESP-IDF (`idf.py`) | IDF component | `integrations/nano-ros/` (`idf_component.yml`, `CMakeLists.txt`, `Kconfig.projbuild`) | [ESP32 (ESP-IDF component)](./integration-esp-idf.md) |
| FreeRTOS, CMake project | `add_subdirectory(nano-ros)` | `cmake/platform/nano-ros-freertos.cmake` | [Build as a CMake subdirectory](./build-as-subdirectory.md), [FreeRTOS (QEMU)](./freertos.md) |
| ThreadX, CMake project | `add_subdirectory(nano-ros)` | `cmake/platform/nano-ros-threadx.cmake` | [Build as a CMake subdirectory](./build-as-subdirectory.md), [ThreadX](./threadx.md) |
| PX4 (`make px4_…`) | `EXTERNAL_MODULES_LOCATION` copy-out | `integrations/px4/module-template/` | [PX4 (integration shell)](./integration-px4.md) |
| PlatformIO (`pio run`) | library + pre-build codegen script | `library.json` (repo root) + `integrations/platformio/` | `integrations/platformio/README.md` in the repo |
| IDE project (no CMake) | emitted drop-in sources | *planned* — no shell ships today | see note below |

Notes on the rows that need them:

- **Vendored FreeRTOS** — STM32Cube, MCUXpresso, Pico SDK, and upstream
  `FreeRTOS-Kernel` all count. Your SDK builds the kernel and network
  stack; your CMake project does `add_subdirectory(nano-ros)` and links
  `NanoRos::NanoRos`. See the honest caveat in
  [FreeRTOS: who builds the kernel?](#freertos-who-builds-the-kernel)
- **PX4** is a *hookless* host: its configure step cannot run nano-ros
  codegen from inside, so codegen runs **ahead of** the PX4 build,
  emitting module directories PX4 then builds natively. PlatformIO works
  the same way, via a pre-build script.
- **IDE hosts** (STM32CubeIDE, MCUXpresso IDE, IAR, Keil) have no build
  hook nano-ros can join. A drop-in emitter (prebuilt static library +
  shim sources + an integration README) is designed but **not shipped
  yet**. Today, the practical route is your IDE's external-CMake or
  makefile support driving the
  [CMake subdirectory](./build-as-subdirectory.md) path.

In every row the division of labor is the same: **your build owns
compile and link; nano-ros owns the adapter shell and the host-time
code generation.** Generated configuration is baked to C at build time
on your host — `system.toml` and launch files never reach the device.

## Two homes for configuration

When you integrate against your own board, you author facts in exactly
two places. The split is by *what kind of fact it is*:

**1. A board package — facts intrinsic to the board.** A directory in
your workspace with a `package.xml` and an `nros-board.toml`: the
board's name, RTOS platform, target triple, architecture, entry shape.
These are true of every copy of that board on every desk, so they are
reusable and shareable. Discovery: the CLI's board catalog scans
`<nano-ros>/packages/boards/` plus every root named by
`NROS_EXTRA_BOARD_PATH` (PATH-style list of directories shaped like
`packages/boards/`), so an out-of-tree board dir is seen without
copying it into the checkout.

**2. An `[image.<id>]` block, and a `[board_config.<board>]` beside it — facts about this checkout, this
machine, this application.** It lives in your bringup package's
`system.toml`, next to your nodes and launch files:

```toml
[image.my-board]
board = "my-board"         # joins to the board package by name
                           # the rustc triple comes from that board's descriptor

[board_config."my-board"]  # site facts: where YOUR SDK lives, which config
sdk.vendor = "{env:VENDOR_SDK_DIR}"     # headers are yours — keyed by BOARD
```

SDK paths, config-header locations, serial ports: anything another
developer's checkout would spell differently belongs here, keyed per
deploy target, and never in the board package.

If a fact would survive being published with the board, it is a board
fact. If it names a path on your disk, it is a deploy fact.

## FreeRTOS: who builds the kernel?

One honest exception to the principle. On the QEMU reference board,
nano-ros **can** build the FreeRTOS kernel and lwIP for you — point
`FREERTOS_DIR` at a kernel checkout and
`cmake/platform/nano-ros-freertos.cmake` compiles it into the image.
That is a convenience for the out-of-the-box demo, and it is the one
place nano-ros compiles an RTOS.

With a **vendored** kernel — STM32Cube's `Middlewares/Third_Party/
FreeRTOS`, MCUXpresso's or the Pico SDK's bundled kernel — do not use
that path. Your SDK's build owns the kernel, its port selection, and
`FreeRTOSConfig.h`; nano-ros compiles against your include directories
and links in as a library, the same as every other row in the table.

## Where to go next

- Zephyr workspace: [Zephyr (west module)](./integration-zephyr.md)
- NuttX board: [NuttX (apps/external)](./integration-nuttx.md)
- ESP-IDF app: [ESP32 (ESP-IDF component)](./integration-esp-idf.md)
- PX4 firmware: [PX4 (integration shell)](./integration-px4.md)
- Plain CMake host (FreeRTOS, ThreadX, POSIX):
  [Build as a CMake subdirectory](./build-as-subdirectory.md)
- Deploy targets and `system.toml`:
  [Deployment Workflow](../user-guide/deployment.md)
