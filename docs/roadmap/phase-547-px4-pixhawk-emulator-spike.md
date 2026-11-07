# Phase 547 — Spike: does PX4's NuttX firmware for a shippable Pixhawk boot in an emulator?

**Status (2026-11-07). OPEN — W1 done; W2 reaches `nsh>` with one firmware
deviation (console TX DMA off); W3: the nano-ros module builds, links and
opens its uORB session on the emulated 6X, then a heap corruption stops it.** A time-boxed spike (about
three working days). It decides the board and emulator for first-class PX4
support. It writes no RFC and changes no shipped code: the RFC amendment and
the implementation phase are drafted from its findings, after the maintainer
settles the design.

## Why

The maintainer wants PX4 supported like the other first-class boards: a board
close to a real PX4 drone, an emulator that runs its tests, and a book chapter.
Today PX4 is supported only as SITL (`px4_sitl_default`, a host process), which
[phase-542](phase-542-px4-supported-platform.md) is making a daily Runtime cell.
SITL stays as the quick-start and dev loop; this spike is about the board.

PX4 runs on NuttX on every Pixhawk. (ChibiOS is ArduPilot's RTOS, not PX4's.)
So the question is not which OS, but which emulator boots PX4's NuttX image on
a board one can buy.

## What is known (researched 2026-11-07)

| Board | MCU | Shippable | Emulator evidence for PX4 on NuttX |
|---|---|---|---|
| Holybro/CUAV **Pixhawk 6X** (`px4_fmu-v6x`) | STM32H753 | yes | none published. Renode boots **ArduPilot (ChibiOS)** on it (ArduPilot's reference Ethernet target, with a patched Renode fork) |
| Holybro **Pixhawk 6C** (`px4_fmu-v6c`) | STM32H743 | yes, often out of stock | Athrill (Hakoniwa): PX4 v1.17 boots and hovers, in an open PR |
| Holybro **Pixhawk 4** (`px4_fmu-v5`) | STM32F765 | **no**, discontinued | Athrill v0.2.0: unmodified PX4 v1.17, released |
| **Pixhawk 6X-RT** (`px4_fmu-v6xrt`) | NXP i.MX RT1176 | yes | only a third-party, hand-written Renode platform |

QEMU has no STM32H7 machine. Stock Renode has a CPU-level `stm32h743.repl`
and no Pixhawk board; known gaps include a coarse PWR model and a flash-address
issue. ArduPilot's Renode models are GPLv3, so nano-ros (MIT OR Apache-2.0)
may read them as a reference but must not vendor them.

**Candidate under test:** Pixhawk 6X on stock Renode 1.17 (already in
`nros-sdk-index.toml` for the Nucleo-F767ZI twin, RFC-0115 D4). It has
Ethernet, which PX4's uXRCE-DDS bridge uses to reach ROS 2 on a companion
computer, so the emulated topology matches a real drone.
**Fallback:** Pixhawk 6C on Athrill.

## Work items

### W1 — Build `px4_fmu-v6x_default` from the pinned PX4-Autopilot (S)

Build stock PX4 (no nano-ros module) with the toolchain PX4 expects. Record the
toolchain, build time, image size and the ELF's memory map.

**Acceptance:** an ELF and its map, the exact command, and the PX4 commit.

### W2 — Boot it in stock Renode to `nsh>` (M)

A minimal Pixhawk 6X platform description built on Renode's STM32H7 CPU
description: memory, clocks, the console UART PX4's NuttX config uses, and
whatever board init needs to get past. Use ArduPilot's published layout only as
a reference. Log every unmodelled peripheral access.

**Acceptance:** either the NuttX `nsh>` prompt over the console UART, or the
list of peripherals board init hangs or faults on, with addresses and the
PX4/NuttX code that touches them.

### W3 — The nano-ros module runs a uORB round trip on the emulated board (M)

Only if W2 reaches `nsh>`. Build the firmware with the nano-ros example module
(`examples/px4/cpp/firmware`) and run a uORB publish/subscribe round trip.

**Acceptance:** the round trip printed in the Renode console log.

### W4 — Stretch: Ethernet to a host XRCE Agent (M)

Renode's Ethernet bridged to the host, PX4's `uxrce_dds_client` (or the
nano-ros XRCE path) reaching a host `MicroXRCEAgent`, and `ros2 topic echo`
seeing the topic.

**Acceptance:** one topic seen from ROS 2 on the host.

### W5 — Fallback: Athrill with `px4_fmu-v6c` (M)

Only if W2 shows Renode needs more model work than the time box allows. Boot
PX4 on Athrill's Pixhawk 6C target and answer the same W2/W3 questions.

### W6 — Findings and recommendation (S)

A findings section in this doc: which board and emulator, what works, what
model work remains and its estimated size, emulation speed (real-time factor),
and what a CI job would cost. The maintainer settles the design from it; the
RFC amendment (RFC-0039 / RFC-0115) and the implementation phase follow.

## Ground rules

- The spike's build trees live outside every git checkout, on the volume with
  room, and are deleted when the phase closes. Nothing under them is committed.
- Platform descriptions and scripts written here are spike artifacts. They move
  into the tree only through the implementation phase.

## Findings

### W1 — stock build (done, 2026-11-07)

`make px4_fmu-v6x_default` at the PX4-Autopilot commit our
`third-party/px4/PX4-Autopilot` submodule pins (dated 2026-04-24; PX4 1.17.0,
NuttX 12.12.0), Arm GNU 13.2.1 from the SDK store, PX4's
`Tools/setup/requirements.txt` in a venv with `PYTHONPATH` cleared: exit 0,
81 s. Text 1,954,824 B, data 5,200 B, bss 98,140 B. The app is linked at
`0x08020000` (after the 128 KiB bootloader); `.data`'s load address ends at
`0x081fe858`.

### W2 — boot in stock Renode 1.17.0 (`nsh>` reached)

Booted with `sysbus LoadBinary <app>.bin 0x08020000` and the vector table at
`0x08020000`, no bootloader. `LoadELF` fails, because it zero-fills `.bss` at
its load address in flash (`0x081fe858 + 0x1775c`, past the 2 MiB bank).

What it took, each found by the boot stopping on it:

| Stop | Cause | Fix (spike artifact) |
|---|---|---|
| polled forever on SPI5 status | base `stm32h7.repl` models SPI4 only | `SPI.STM32H7_SPI` at SPI1/2/3/5/6 |
| `failed to initialize mtd driver`, endlessly | nothing answers the FRAM's RDID | a 32 KiB FM25V02A model (C#, ~120 lines) on SPI5, framed by its chip select PG7. Renode's H7 SPI ends a "transmission" after every byte, so only the CS delimits a command |
| `failed to initialize EEPROM driver` | no 24LC64 on I2C3/I2C4 | Renode's `Memory.GenericI2cEeprom`, `addressBitSize: 16`, `pageSize: 32`, backed by `MappedMemory` so a reboot keeps it (the default 8-bit addressing made `netman` reboot forever) |
| silent after the EEPROM probe | Renode's `STM32F7_I2C` (sealed) drops 16-bit accesses; NuttX reads ISR/RXDR and writes TXDR 16 bits wide | a width adapter at the real I2C3/I2C4 addresses, forwarding to the stock model moved to a spare address |
| polled forever on SDMMC2 status | the base models SDMMC1 only | `SD.STM32HSDMMC` at SDMMC2, IRQ 124, plus a 64 MiB FAT image. **NuttX still does not detect the card** (`mount failed: No such device`); not yet investigated |
| `HW arch: PX4_FMU_V6X` printed ~5 million times | **USART3 TX DMA**: NuttX's `up_dma_txcallback` credits the bytes sent only when the DMA status carries the transfer-complete flag. Renode's `STM32DMA` raises the interrupt without it, and also leaves the stream's EN bit set at `NDTR = 0`, so the driver resends one buffer forever | **diagnostic only:** `CONFIG_USART3_TXDMA` off in the scratch build. The real fix is in the DMA model (it is sealed, so a replacement stream model, or an upstream fix) |

With those, the firmware boots through the full `rcS` to **`NuttShell (NSH)
NuttX-12.12.0` / `nsh>`**: the version banner, parameters imported from the
emulated FRAM, `uorb`, `logger`, `ekf2`, `dataman` (which fails without the SD
card), MAVLink on `/dev/ttyS6` and **on Ethernet (UDP 14550)** through Renode's
Synopsys DWC QoS MAC, and `uavcan`.

What fails, as expected without models:
- every sensor (`bmi088`, `icm42688p`, `icm20649`, `bmm150`, `ist8310`,
  `bmp388`: "no device on bus");
- PX4IO (the STM32F103 I/O co-processor on USART6);
- the SD card (above) and so `dataman` and logging;
- the hardware-revision ADC reads 0 (`V6X000`).

**Not yet measured:** the real-time factor. A boot to `nsh>` took well
under 220 s of host time.

### What this means so far

Stock Renode carries the Pixhawk 6X much further than the research
suggested. The model work so far is one small device model (FRAM), one access
adapter (I2C) and configuration: every remaining stop was a missing
instance or a width or framing mismatch, not a missing peripheral class.
Only one deviation from the hardware firmware remains, the console TX DMA, and
it is a defect in Renode's DMA model.

### W3 — the nano-ros uORB module on the emulated 6X (in progress)

Nothing in the tree built the module for a flight controller before this
(phase-542 W8.b deferred it). Reaching a session took the following; each is
a finding against nano-ros, not PX4, unless marked:

| Stop | Cause | Spike measure (fix it implies) |
|---|---|---|
| `nros-c`'s size probe fails, `_SC_HOST_NAME_MAX` | its nested `std` build needs the NuttX-patched `libc` | initialise `third-party/nuttx/libc` (no change) |
| — | no recipe cross-builds `nros-cpp` for a PX4 FMU | `cargo build -p nros-cpp --target thumbv7em-nuttx-eabihf -Zbuild-std=core,alloc,panic_abort`, features `alloc,log,panic-platform,rmw-cffi,platform-nuttx`, `NUTTX_DIR` = PX4's NuttX tree (its `config.h` is generated there). Builds in 8 m 49 s |
| `timer.c`: `SIGEV_THREAD` undeclared | PX4's fmu-v6x NuttX has no `CONFIG_SIG_EVTHREAD`; our NuttX boards enable it | timer functions stubbed. Fix: a work-queue timer backend for PX4, or a required NuttX fragment |
| `nros_platform_panic` unresolved | no board provides it on PX4 | stubbed. Fix: the PX4 integration provides it |
| `<atomic>`, `<type_traits>`, `<new>`, `<chrono>` not found | PX4 compiles C++ with `-nostdinc++` against NuttX's C-only `include/cxx` | `__atomic` builtins in `nros-rmw-uorb` (`session.cpp`, `subscriber.cpp`); the riscv64-threadx `cxx-compat` shim on the module's include path; PX4's `hrt_absolute_time()` in `sched_task_work_item.hpp`. Fix: a shared freestanding shim for PX4 NuttX |
| `macro "link" requires 2 arguments` | NuttX's `unistd.h` defines `link(p1, p2)`; `rclcpp/future.hpp` has a member `link()` | renamed `link_front`. Fix: rename, plus a gate for NuttX/libc macro names in public headers |
| `-Wcast-align` error | PX4 builds with `-Wcast-align -Werror`; `px4_callback_glue.cpp` casts `unsigned char*` to an aligned type | cast through `void *` |
| `inline variables … C++17` | PX4 builds modules as `gnu++14` | `CXX_STANDARD 17` on the module target (a `-std=` flag loses to the property) |
| `FLASH overflowed by 297,844 B` | stock 6X firmware already fills 99 % of its 1920 KiB; `libnros_cpp.a` at `opt-level=3` | `opt-level=z`, fat LTO, `codegen-units=1`: nano-ros then costs **207,845 B** of flash (`libnros_cpp.a` 195,003 B, the largest archive in the image, above `ekf2`'s 166,517 B; module 6,734 B; platform 6,108 B). Room made by dropping UAVCAN, the ICM-42688P and GPS drivers and temperature compensation (none can run in the emulator). `.bss` grows from 96 KiB to 198 KiB |
| UsageFault in `arm_dma_initialize`, before the first `[boot]` | **`.nros_boot_config` is an orphan section** to PX4's linker script; `ld` placed it at `_eronly`, so NuttX copied it into `.data` and every initialised global moved by 0x10c (Zephyr has `zephyr/nros_boot_config.ld`; NuttX/PX4 have nothing) | `KEEP(*(.nros_boot_config))` in the scratch `script.ld`. Fix: a section name every stock script absorbs (`.rodata.nros_boot_config`), found by symbol rather than section. Affects every NuttX/PX4 target whose script we do not own |
| typed `nros_uorb_demo start` not seen | not investigated (the console's RX DMA, or typed before the prompt) | started from `rc.board_sensors` instead |
| **MemManage fault in `mm_free`** (task `netinit`, freeing `0xffffff90`) just after `nros: session open (rmw=uorb)` | **heap corruption**; open | not a work-queue stack overflow: the same fault with `wq:rate_ctrl` at 24,000 B (PX4 caps that option at 10,000 B, itself worth noting — the generated entry runs nano-ros on `rate_ctrl`, 3,150 B by default). **Leading suspect:** the cross-built Rust side sizes NuttX `pthread` storage from a size probe over a default NuttX config, while PX4's NuttX 12.12 config may make `pthread_mutex_t` / condvar larger (e.g. priority inheritance) — a `repr(C)` mirror mismatch. Next: compare `sizeof` of each platform type under PX4's `config.h` with what the probe baked |

The scratch edits are saved as `px4-scratch-patches.diff` (PX4) and
`nros-spike-patches.diff` (nano-ros) in the spike's build directory, outside
the repository.

### Where this leaves the board decision

The emulator side holds up: stock Renode plus small models runs stock PX4 for
the 6X to `nsh>`, and runs nano-ros's module far enough to open a uORB session.
Every stop since has been a nano-ros portability defect that a real Pixhawk
would hit identically, which is what the twin is for. The open question has
moved from "can it be emulated" to "how much nano-ros work makes PX4-on-NuttX a
supported target", and the table above is that list.
