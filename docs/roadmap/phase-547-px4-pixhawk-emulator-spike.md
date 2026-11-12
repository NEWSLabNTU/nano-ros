# Phase 547 — Spike: does PX4's NuttX firmware for a shippable Pixhawk boot in an emulator?

**Status (2026-11-10). OPEN. W1 done; W2 reaches `nsh>` with the UART DMA
of the console and two telemetry ports off (one Renode DMA defect); the whole
spike is now scripted (see "Reproducing the spike"). W3: the `mm_free` fault is
confirmed as the module's main-task stack (issue 2398) and gone with both
thread stacks raised; nano-ros publishes and stock PX4 reads it, and since
issue 2403's fix (a uORB subscription held by pointer, not by a per-task-group
fd) the subscription half receives `vehicle_status` too.** A time-boxed spike (about
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

- **Status:** DONE (2026-11-07)

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
| typing at `nsh>` is lost; ~30 s into virtual time the emulation all but stops (found 2026-11-10) | the same `STM32DMA` defect on the other DMA-backed UARTs: USART3 RX, and UART7 (MAVLink on `/dev/ttyS6`) whose first TX DMA start never returns (every sampled PC on one store in `stm32_sdma_start`) | **diagnostic only:** `CONFIG_USART3_RXDMA`, `CONFIG_UART5_{RX,TX}DMA`, `CONFIG_UART7_{RX,TX}DMA` off too. Same model fix |

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

**Real-time factor** (measured 2026-11-10, with every DMA-backed UART above
off): about 0.87, 69.9 s of virtual time in 80.3 s of host time; `nsh>` 30 s
after Renode starts.

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
| typed `nros_uorb_demo start` not seen | the console's RX DMA (the W2 DMA defect; measured 2026-11-10) | started from `rc.board_sensors`; with `CONFIG_USART3_RXDMA` off, typing works |
| **MemManage fault in `mm_free`** (task `netinit`, freeing `0xffffff90`) just after `nros: session open (rmw=uorb)` | **the module's main task overflowing its stack** (PX4's default `STACK_MAIN`, 2048 B, against a measured 15.3 KB open path; a NuttX task stack is a heap block); confirmed 2026-11-10 by the scripted repro, [issue 2398](../issues/archived/2398-px4-thread-stacks-below-executor-floor.md) | reproduced exactly with the stock 2048 B, gone with `STACK_MAIN 16384`. With only that raised, `wq:rate_ctrl`'s stock 3150 B overflows next (a HardFault in `ekf2`), so `CONFIG_WQ_RATE_CTRL_STACKSIZE=8192` too: then no fault in 70 s. Fix: 2398's configure-time refusal of both stock values (merged). The size-mismatch suspect was ruled out by measurement, [issue 2397](../issues/archived/2397-px4-nuttx-platform-type-sizes.md) (resolved) |
| `vehicle_status` never reaches the demo (`status=0`), while PX4's `listener` shows commander publishing it | the subscription's handle is a NuttX fd, opened by the transient `start` task; on the `wq:rate_ctrl` thread (another task group) `orb_check` is EBADF, and the C shim read that error as "no data" | resolved: [issue 2403](../issues/archived/2403-px4-uorb-subscription-never-taken.md) holds the subscription by pointer on the PX4 build; the demo then counts `status=51` at 30 ticks. The error-as-no-data half is [issue 2410](../issues/archived/2410-cffi-poll-swallows-backend-error.md) |

The scratch edits are saved as `px4-scratch-patches.diff` (PX4) and
`nros-spike-patches.diff` (nano-ros) in `/mnt/wd/nt/p547/repro/`, outside the
repository, and inlined in the next section. (The first spike's copies were
lost with its host.)

### Reproducing the spike (scripted, 2026-11-10)

The first spike's scratch (its patches and its Renode `.repl`/`.resc`) was on no
host anyone could find, so it was rebuilt from this doc and every step is now a
script. The scripts live under `/mnt/wd/nt/p547/repro/` (outside every
checkout, per the ground rules); this section carries their full text, so the
doc alone rebuilds them. Inputs: PX4-Autopilot at our submodule pin
(`d6f12ad1c4f70ad3230afd7d86e971421e02fef4`, v1.17.0) cloned with its NuttX
submodules at `/mnt/wd/nt/p547/PX4-Autopilot`, a venv with PX4's
`Tools/setup/requirements.txt` at `/mnt/wd/nt/p547/venv`, Renode 1.17.0 from the
SDK store (`[tool.renode]`, installed by `nros setup`), Arm GNU 13.2.1 from the
store, and nano-ros at `33c43f9fc1c05f1d902f7a70dc5e274e20ffbab3`. Every step
runs inside the `ros2` distrobox (`repro/box`) and is idempotent.

| script | does |
|---|---|
| `env.sh`, `box` | shared paths; run a step inside the box |
| `10-px4-stock-build.sh` | W1: stock `px4_fmu-v6x_default` from the pristine clone (`build-v6x`) |
| `20-px4-scratch.sh` | `PX4-scratch`: a worktree of the pristine clone, each initialised submodule cloned `--shared` from it (no network), plus `px4-scratch-patches.diff` |
| `22-px4-w2-build.sh` | the W2 image: stock label from `PX4-scratch` (`build-w2`); also configures NuttX in-source, which the nano-ros cross-build reads |
| `renode/pixhawk6x.repl`, `.resc`, `FM25V02A.cs`, `I2cWidthAdapter.cs` | the platform, boot script and the two small models W2 names |
| `21-renode-run.sh`, `send.py` | boot an image headless, wait for a console pattern or a timeout, stop that Renode by PID; optionally type console commands (`SEND`) or run monitor commands (`PROBE`) |
| `30-nros-scratch.sh` | `nros-scratch`: a detached nano-ros worktree (never the tracked tree), `third-party/nuttx/libc`, plus `nros-spike-patches.diff` |
| `31-nros-cross.sh` | `libnros_cpp.a` for `thumbv7em-nuttx-eabihf` at `opt-level=z`, fat LTO, one codegen unit; the headers placed from the same cargo run; the C platform archive |
| `40-px4-nros-build.sh` | the W3 image: `PX4-scratch` with the firmware example module from `nros-scratch`, a board label and an optional `STACK_MAIN` |
| `run-all.sh` | all of the above in order, ending with both experiments |

Measured on this rerun: W1 builds in 105 s (text 1,954,768 B, 56 B less than the
first spike's, same commit); the W2 image reaches `nsh>` 30 s after Renode
starts. The W3 images fit without `FLASH` overflow (text 1,945,476 B, `.bss`
199,576 B) once the four drivers are dropped.

**Three findings the first spike did not record:**

- **USART3 RX DMA, UART5 and UART7 DMA hit the same Renode `STM32DMA` defect as
  the console's TX DMA** (W2's table). With USART3's RX DMA on, typed input is
  lost (why "typed `nros_uorb_demo start` not seen") and `dma2` takes millions
  of register writes. When MAVLink starts on `/dev/ttyS6` (UART7, TX DMA) about
  30 s into virtual time, every sampled PC sits on one store in
  `stm32_sdma_start` and virtual time stops advancing (31.8 s of virtual time in
  293 s of host time). With all of them off in the scratch `defconfig`, typing
  works and the board runs at a **real-time factor of about 0.87** (69.9 s of
  virtual time in 80.3 s of host time), which answers W6's speed question for
  now. The model fix is still the DMA stream, not the firmware.
- **Renode's own log** must be quietened (`logLevel 3` on `dma1`, `dma2`,
  `nvic`, `usart6`): an unfiltered run wrote 2.2 GB in six minutes.
- **No SD card is modelled** in this rerun; NuttX did not detect the first
  spike's 64 MiB image either (`mount failed: No such device` both ways).

<details><summary>run-all.sh</summary>

```bash
#!/bin/bash
# phase-547 repro, from nothing to the two experiments, in order. Run on the HOST;
# each step enters the ros2 box itself. Every step is idempotent.
set -euo pipefail
R=/mnt/wd/nt/p547/repro; P=/mnt/wd/nt/p547; B="$R/box"
"$B" "$R/10-px4-stock-build.sh"                    # W1: pristine stock build
bash "$R/20-px4-scratch.sh"                         # PX4-scratch + px4-scratch-patches.diff
"$B" "$R/22-px4-w2-build.sh"                        # W2 image (also configures NuttX)
"$B" "$R/21-renode-run.sh" "$P/build-w2/px4_fmu-v6x_default.bin" "$R/run-w2" 'nsh> ' 300
bash "$R/30-nros-scratch.sh" 33c43f9fc1c05f1d902f7a70dc5e274e20ffbab3   # + nros-spike-patches.diff
"$B" "$R/31-nros-cross.sh"                          # libnros_cpp.a, placed headers, platform .a
"$B" "$R/40-px4-nros-build.sh" nros "$P/build-nros"            # stock STACK_MAIN (2048)
"$B" "$R/21-renode-run.sh" "$P/build-nros/px4_fmu-v6x_nros.bin" "$R/run-nros-default" 'up_assert' 300
"$B" "$R/40-px4-nros-build.sh" wqstk "$P/build-wqstk" 16384    # experiment 1
"$B" env SEND='listener debug_key_value;listener vehicle_status;nros_uorb_demo status' \
  "$R/21-renode-run.sh" "$P/build-wqstk/px4_fmu-v6x_wqstk.bin" "$R/run-wqstk" 'value=29|up_assert' 420
```

</details>

<details><summary>env.sh and box</summary>

```bash
# phase-547 repro -- shared settings. Sourced by every step script (inside the box).
P547=${P547:-/mnt/wd/nt/p547}
REPRO=$P547/repro
# A nano-ros checkout to read (never patched): the worktree this session uses.
NROS_SRC=${NROS_SRC:-/mnt/wd/data/projects/nano-ros-box2/.claude/worktrees/p546v3}
PX4_PRISTINE=$P547/PX4-Autopilot          # PX4 v1.17.0 at our submodule pin, untouched
PX4_SCRATCH=$P547/PX4-scratch             # worktree + shared submodule clones, patched
NROS_SCRATCH=$P547/nros-scratch           # nano-ros worktree, patched
RENODE=${RENODE:-$HOME/.nros-box/sdk/renode/1.17.0/renode_1.17.0-portable/renode}
NIGHTLY=nightly-2026-04-11
export TMPDIR=/mnt/wd/nt GIT_CEILING_DIRECTORIES=/mnt/wd GIT_SUBMODULES_ARE_EVIL=1
unset PYTHONPATH
px4_venv() { . "$P547/venv/bin/activate"; }

# --- box ---
#!/bin/bash
# usage: repro/box <script> [args...]   -- run a repro step inside the ros2 distrobox
exec env DBX_CONTAINER_MANAGER=docker distrobox enter ros2 -- bash -lc "cd /mnt/wd/data/projects/nano-ros-box2/.claude/worktrees/p546v3 && . scripts/dev/ros2-box-env.sh && NROS_QUIET_ACTIVATE=1 source ./activate.sh >/dev/null 2>&1; export TMPDIR=/mnt/wd/nt GIT_CEILING_DIRECTORIES=/mnt/wd; $(printf '%q ' "$@")"
```

</details>

<details><summary>10-px4-stock-build.sh and 22-px4-w2-build.sh</summary>

```bash
#!/bin/bash
# W1 -- build stock px4_fmu-v6x_default from the pristine PX4 clone. Idempotent
# (cmake re-configure is a no-op, ninja rebuilds only what changed).
set -euo pipefail
. "$(dirname "$0")/env.sh"
px4_venv
B=$P547/build-v6x
cmake -S "$PX4_PRISTINE" -B "$B" -G Ninja -DCONFIG=px4_fmu-v6x_default >"$B.configure.log" 2>&1 \
  || { tail -30 "$B.configure.log"; exit 1; }
time ninja -C "$B"
arm-none-eabi-size "$B/px4_fmu-v6x_default.elf"
ls -l "$B/px4_fmu-v6x_default.bin"

# --- 22-px4-w2-build.sh ---
#!/bin/bash
# W2 image -- stock px4_fmu-v6x_default from PX4-scratch (the only firmware
# deviation that matters to W2 is CONFIG_USART3_TXDMA off; the KEEP line and the
# rc.board_sensors start line are inert without the nano-ros module).
set -euo pipefail
. "$(dirname "$0")/env.sh"
px4_venv
B=$P547/build-w2
cmake -S "$PX4_SCRATCH" -B "$B" -G Ninja -DCONFIG=px4_fmu-v6x_default >"$B.configure.log" 2>&1 \
  || { tail -30 "$B.configure.log"; exit 1; }
ninja -C "$B" >"$B.build.log" 2>&1 || { tail -40 "$B.build.log"; exit 1; }
arm-none-eabi-size "$B/px4_fmu-v6x_default.elf"
```

</details>

<details><summary>20-px4-scratch.sh and 30-nros-scratch.sh</summary>

```bash
#!/bin/bash
# Make PX4-scratch: a git worktree of the pristine PX4 clone, with each
# initialised submodule cloned --shared from the pristine tree (no network), then
# apply px4-scratch-patches.diff. The pristine clone stays untouched (W1).
# Idempotent: re-running skips what exists and re-applies nothing twice.
set -euo pipefail
. "$(dirname "$0")/env.sh"
if [ ! -e "$PX4_SCRATCH/.git" ]; then
  git -C "$PX4_PRISTINE" worktree add --detach "$PX4_SCRATCH" HEAD
fi
# Parents before children (pymavlink lives inside mavlink).
git -C "$PX4_PRISTINE" submodule status --recursive | awk '$1 !~ /^-/ {print $1, $2}' |
  sed 's/^[+U ]//' | sort -k2 | while read -r sha path; do
    if [ ! -e "$PX4_SCRATCH/$path/.git" ]; then
      rmdir "$PX4_SCRATCH/$path" 2>/dev/null || true
      git clone -q --shared --no-checkout "$PX4_PRISTINE/$path" "$PX4_SCRATCH/$path"
      git -C "$PX4_SCRATCH/$path" checkout -q "$sha"
      echo "cloned $path @ $sha"
    fi
  done
P=$REPRO/px4-scratch-patches.diff
if git -C "$PX4_SCRATCH" apply --check -R "$P" 2>/dev/null; then
  echo "px4-scratch-patches.diff already applied"
else
  git -C "$PX4_SCRATCH" apply --whitespace=nowarn "$P"
  echo "applied px4-scratch-patches.diff"
fi

# --- 30-nros-scratch.sh ---
#!/bin/bash
# Make nros-scratch: a detached worktree of nano-ros (never the tracked tree),
# init the one submodule the NuttX cross-build needs, apply nros-spike-patches.diff.
# usage: 30-nros-scratch.sh [commit]   (default: origin/main; the run used 33c43f9fc1)
set -euo pipefail
. "$(dirname "$0")/env.sh"
REV=${1:-origin/main}
if [ ! -e "$NROS_SCRATCH/.git" ]; then
  git -C "$NROS_SRC" worktree add --detach "$NROS_SCRATCH" "$REV"
fi
# nros-c's size probe runs a nested std build that needs the NuttX-patched libc.
git -C "$NROS_SCRATCH" submodule update --init third-party/nuttx/libc
P=$REPRO/nros-spike-patches.diff
if git -C "$NROS_SCRATCH" apply --check -R "$P" 2>/dev/null; then
  echo "nros-spike-patches.diff already applied"
else
  git -C "$NROS_SCRATCH" apply "$P"; echo "applied nros-spike-patches.diff"
fi
```

</details>

<details><summary>31-nros-cross.sh</summary>

```bash
#!/bin/bash
# Cross-build nros-cpp for the fmu-v6x (thumbv7em-nuttx-eabihf) from the PATCHED
# scratch nano-ros, against PX4-scratch's configured NuttX tree, at the spike's
# flash-fitting profile (opt-level=z, fat LTO, codegen-units=1). Places the
# per-build headers from the SAME cargo run (issue 1824), so the module can
# never pair this archive with another build's sizes header (issue 2397 suspect 1).
# Then builds the C platform archive (nros-platform-posix sources, NuttX headers).
set -euo pipefail
. "$(dirname "$0")/env.sh"
N=$PX4_SCRATCH/platforms/nuttx/NuttX/nuttx
[ -f "$N/include/nuttx/config.h" ] || { echo "configure a PX4-scratch build first (22-*.sh)"; exit 1; }
GI=$(arm-none-eabi-gcc -print-file-name=include)
CPU="-mcpu=cortex-m7 -mthumb -mfloat-abi=hard -mfpu=fpv5-d16"
export NUTTX_DIR=$N
export CC_thumbv7em_nuttx_eabihf=arm-none-eabi-gcc AR_thumbv7em_nuttx_eabihf=arm-none-eabi-ar
export CFLAGS_thumbv7em_nuttx_eabihf="$CPU -nostdinc -isystem $N/include -isystem $GI -D__NuttX__"
export CARGO_TARGET_DIR=$P547/cargo-repro
export CARGO_PROFILE_RELEASE_OPT_LEVEL=z CARGO_PROFILE_RELEASE_LTO=fat CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1
OUT=$P547/repro/out; mkdir -p "$OUT"
cd "$NROS_SCRATCH"
python3 scripts/build/cargo-out-dir-headers.py --package nros-cpp --package nros-c --dest "$OUT/placed" -- \
  cargo +$NIGHTLY build -p nros-cpp --target thumbv7em-nuttx-eabihf \
    -Zbuild-std=core,alloc,panic_abort --no-default-features \
    --features alloc,log,panic-platform,rmw-cffi,platform-nuttx --release \
    --message-format=json-render-diagnostics
cp -f "$CARGO_TARGET_DIR/thumbv7em-nuttx-eabihf/release/libnros_cpp.a" "$OUT/libnros_cpp.a"
# Platform archive: the posix C port, compiled against PX4's NuttX headers.
P=packages/platform/nros-platform-posix/src
rm -rf "$OUT/platform"; mkdir -p "$OUT/platform"
for f in platform timer termination px4_spike_stubs; do
  arm-none-eabi-gcc $CPU -Os -ffunction-sections -fdata-sections -std=gnu11 -nostdinc \
    -isystem "$N/include" -isystem "$GI" -D__NuttX__ \
    -I packages/platform/nros-platform-api/include -c "$P/$f.c" -o "$OUT/platform/$f.o"
done
rm -f "$OUT/libnros_platform_nuttx.a"
arm-none-eabi-ar rcs "$OUT/libnros_platform_nuttx.a" "$OUT"/platform/*.o
ls -l "$OUT"/*.a
```

</details>

<details><summary>40-px4-nros-build.sh</summary>

```bash
#!/bin/bash
# W3 image -- PX4-scratch fmu-v6x with the nano-ros uORB demo module from the
# patched scratch nano-ros (examples/px4/cpp/firmware), started from
# rc.board_sensors. Needs 31-nros-cross.sh first.
# usage: 40-px4-nros-build.sh <label: nros|wqstk> <build-dir> [STACK_MAIN bytes]
set -euo pipefail
. "$(dirname "$0")/env.sh"
LABEL=$1; B=$2; STK=${3:-}
OUT=$P547/repro/out
# The CLI the module's codegen runs must be the scratch checkout's own
# (a foreign one is refused, RFC-0090): build it with `just setup-cli` there.
NROS_CLI=$NROS_SCRATCH/packages/cli/target/release/nros
[ -x "$NROS_CLI" ] || ( cd "$NROS_SCRATCH" && NROS_QUIET_ACTIVATE=1 source ./activate.sh >/dev/null 2>&1; just setup-cli )
# The launch resolver: the scratch has no play_launch submodule, so borrow the
# source checkout's, copied OUTSIDE every checkout (issue 1280 would re-root it).
mkdir -p "$OUT/bin"
cp -u "$NROS_SRC/packages/cli/nros-launch-resolve/target/release/nros-launch-resolve" "$OUT/bin/"
export NROS_LAUNCH_RESOLVE=$OUT/bin/nros-launch-resolve
( cd "$NROS_SCRATCH/examples/px4/cpp/firmware" && "$NROS_CLI" configure >"$B.nros-configure.log" 2>&1 ) \
  || { tail -20 "$B.nros-configure.log"; exit 1; }
px4_venv
cmake -S "$PX4_SCRATCH" -B "$B" -G Ninja -DCONFIG=px4_fmu-v6x_$LABEL \
  -DEXTERNAL_MODULES_LOCATION="$NROS_SCRATCH/examples/px4/cpp/firmware" \
  -DNROS_REPO_DIR="$NROS_SCRATCH" \
  -DNROS_CPP_ARCHIVE="$OUT/libnros_cpp.a" \
  -DNROS_PX4_HEADER_DIR="$OUT/placed" \
  -DNROS_PLATFORM_ARCHIVE="$OUT/libnros_platform_nuttx.a" \
  -DNROS_SPIKE_STACK_MAIN="$STK" >"$B.configure.log" 2>&1 \
  || { tail -40 "$B.configure.log"; exit 1; }
ninja -C "$B" >"$B.build.log" 2>&1 || { grep -E 'error|overflow' "$B.build.log" | grep -v -- ' -c /' | head -20; exit 1; }
arm-none-eabi-size "$B/px4_fmu-v6x_$LABEL.elf"
grep -hF '"nros_uorb_demo"' "$B/NuttX/px4.bdat"
```

</details>

<details><summary>21-renode-run.sh and send.py</summary>

```bash
#!/bin/bash
# Boot an image on the Pixhawk 6X model, headless. Waits until the console log
# matches PATTERN or TIMEOUT seconds pass, then stops the Renode it started (by PID).
# usage: 21-renode-run.sh <image.bin> <outdir> [pattern] [timeout-s] [extra monitor cmds]
# SEND="cmd1;cmd2" types those commands into the console socket (port 3456) once
# `nsh> ` first appears, 3 s apart. PROBE="cmd1;cmd2" runs monitor commands over
# the monitor (fed through <outdir>/monitor.in) just before the stop; the answers
# land in <outdir>/renode.log.
set -uo pipefail
. "$(dirname "$0")/env.sh"
BIN=$1; OUT=$2; PAT=${3:-'nsh> '}; TMO=${4:-600}; EXTRA=${5:-}
SEND=${SEND:-}; PROBE=${PROBE:-}
mkdir -p "$OUT"; : > "$OUT/console.log"
# The monitor reads stdin and quits on EOF, so feed it a file that never ends;
# appending a line to monitor.in runs it as a monitor command.
: > "$OUT/monitor.in"
tail -f "$OUT/monitor.in" | "$RENODE" --disable-gui --plain --console \
  -e "\$bin=@$BIN; \$uartlog=@$OUT/console.log; include @$REPRO/renode/pixhawk6x.resc; $EXTRA start" \
  > "$OUT/renode.log" 2>&1 &
PID=$!; echo "$PID" > "$OUT/renode.pid"; t0=$(date +%s); sent=
while kill -0 "$PID" 2>/dev/null; do
  if [ -n "$SEND" ] && [ -z "$sent" ] && grep -a -q 'nsh> ' "$OUT/console.log"; then
    sent=1; sleep 5; IFS=';' read -ra cmds <<< "$SEND"
    python3 "$REPRO/send.py" 3456 3 "${cmds[@]}" &
  fi
  if grep -a -q -E "$PAT" "$OUT/console.log"; then echo "MATCH after $(( $(date +%s) - t0 )) s"; break; fi
  if [ $(( $(date +%s) - t0 )) -ge "$TMO" ]; then echo "TIMEOUT after $TMO s"; break; fi
  sleep 5
done
if [ -n "$PROBE" ]; then
  IFS=';' read -ra pc <<< "$PROBE"
  for c in "${pc[@]}"; do echo "$c" >> "$OUT/monitor.in"; sleep 2; done
fi
sleep 2; kill "$PID" 2>/dev/null; wait "$PID" 2>/dev/null
echo "console: $OUT/console.log ($(wc -l < "$OUT/console.log") lines); renode log: $OUT/renode.log"

# --- send.py ---
import socket, sys, time
# usage: send.py <port> <gap-s> <cmd>...  -- type each command (char by char, CR),
# print whatever comes back (the Renode monitor answers on its telnet port).
port, gap = int(sys.argv[1]), float(sys.argv[2])
s = socket.create_connection(("127.0.0.1", port), timeout=10)
s.settimeout(0.2)
def drain():
    out = b""
    try:
        while True:
            d = s.recv(65536)
            if not d: break
            out += d
    except socket.timeout:
        pass
    return out
time.sleep(1); drain()
for c in sys.argv[3:]:
    for ch in (c + "\r"):
        s.sendall(ch.encode()); time.sleep(0.05)
    time.sleep(gap)
    sys.stdout.write(drain().decode("utf-8", "replace")); sys.stdout.flush()
s.close()
```

</details>

<details><summary>renode/pixhawk6x.repl</summary>

```text
// phase-547 W2 -- Holybro/CUAV Pixhawk 6X (STM32H753) on stock Renode 1.17.0.
// Built on Renode's own CPU description; ArduPilot's GPLv3 layout was a reference only.
using "platforms/cpus/stm32h753.repl"

// --- SPI: the base models SPI4 only; NuttX polls each enabled bus's status ---
spi1: SPI.STM32H7_SPI @ sysbus 0x40013000
spi2: SPI.STM32H7_SPI @ sysbus 0x40003800
spi3: SPI.STM32H7_SPI @ sysbus 0x40003C00
spi5: SPI.STM32H7_SPI @ sysbus 0x40015000
spi6: SPI.STM32H7_SPI @ sysbus 0x58001400

// FM25V02A F-RAM (PX4 parameters) on SPI5, chip select PG7.
fram: SPI.FM25V02A @ spi5

gpioPortG:
    7 -> fram@0

// --- I2C: NuttX makes 16-bit accesses the stock model drops; move the stock
// model to a spare address and put a width adapter at the real one ---
i2c3: @ sysbus 0x4000E000
i2c4: @ sysbus 0x4000E400

i2c3adapter: Miscellaneous.I2cWidthAdapter @ sysbus 0x40005C00
    target: 0x4000E000

i2c4adapter: Miscellaneous.I2cWidthAdapter @ sysbus 0x58001C00
    target: 0x4000E400

// 24LC64 EEPROMs (8 KiB, 16-bit addressing, 32 B pages), backed by MappedMemory
// so a reboot keeps their contents (8-bit addressing made netman reboot forever).
eepromBaseMem: Memory.MappedMemory @ none
    size: 0x2000

eepromImuMem: Memory.MappedMemory @ none
    size: 0x2000

eepromBase: Memory.GenericI2cEeprom @ i2c3 0x51
    memory: eepromBaseMem
    addressBitSize: 16
    pageSize: 32

eepromImu: Memory.GenericI2cEeprom @ i2c4 0x50
    memory: eepromImuMem
    addressBitSize: 16
    pageSize: 32

// --- SDMMC2: the base models SDMMC1 only ---
sdmmc2: SD.STM32HSDMMC @ sysbus 0x48022400
    IRQ -> nvic@124
```

</details>

<details><summary>renode/pixhawk6x.resc</summary>

```text
# phase-547 W2 -- boot a PX4 fmu-v6x image (no bootloader) on the Pixhawk 6X model.
# Variables (set before `include`): $bin (the .bin), $uartlog (console log file).
# Renode cannot join a variable to a path, so the model paths are absolute.
$bin?=@/mnt/wd/nt/p547/build-w2/px4_fmu-v6x_default.bin
$uartlog?=@/mnt/wd/nt/p547/repro/console.log

include @/mnt/wd/nt/p547/repro/renode/FM25V02A.cs
include @/mnt/wd/nt/p547/repro/renode/I2cWidthAdapter.cs

mach create "pixhawk6x"
machine LoadPlatformDescription @/mnt/wd/nt/p547/repro/renode/pixhawk6x.repl

# Silence the per-access warnings that flood the log once PX4 runs (dma2 stream
# writes from the console's RX DMA, the D-cache clean loop, PX4IO's USART6 retries):
# at millions of lines they slow the emulation to a crawl (a 2.2 GB log in 6 min).
logLevel 3 sysbus.dma1
logLevel 3 sysbus.dma2
logLevel 3 sysbus.nvic
logLevel 3 sysbus.usart6

# The console is USART3 (57600 8N1). File backend for the log, a socket for typing.
sysbus.usart3 CreateFileBackend $uartlog true
emulation CreateServerSocketTerminal 3456 "console" false
connector Connect sysbus.usart3 console

macro reset
"""
    sysbus LoadBinary $bin 0x08020000
    cpu VectorTableOffset 0x08020000
"""
runMacro $reset
```

</details>

<details><summary>renode/FM25V02A.cs (the FRAM model)</summary>

```csharp
// phase-547 W2 -- Cypress/Ramtron FM25V02A 32 KiB SPI F-RAM, enough for NuttX's
// drivers/mtd/ramtron.c (RDID, WREN/WRDI, RDSR/WRSR, READ/FSTRD, WRITE).
// Renode's STM32H7_SPI ends a "transmission" after every byte, so the command
// frame is delimited by the chip select (PG7 on the Pixhawk 6X), wired as GPIO 0.
// Contents live in this object, so they survive a machine reset.
using System;
using Antmicro.Renode.Core;
using Antmicro.Renode.Logging;
using Antmicro.Renode.Peripherals.SPI;

namespace Antmicro.Renode.Peripherals.SPI
{
    public class FM25V02A : ISPIPeripheral, IGPIOReceiver
    {
        public FM25V02A()
        {
            memory = new byte[Size];
            for(var i = 0; i < Size; i++) { memory[i] = 0xFF; }
            Reset();
        }

        public void Reset()
        {
            EndFrame();
            writeEnabled = false;
        }

        // CS is active low: low starts a command frame, high ends it.
        public void OnGPIO(int number, bool value)
        {
            if(number != 0) { return; }
            if(!value) { selected = true; EndFrame(); }
            else { selected = false; EndFrame(); }
        }

        public void FinishTransmission()
        {
            // Not a frame boundary on the H7 SPI model (called per byte); the CS is.
        }

        public byte Transmit(byte data)
        {
            if(!selected) { return 0xFF; }
            var idx = index++;
            if(idx == 0)
            {
                command = data;
                switch(command)
                {
                    case 0x06: writeEnabled = true; break;   // WREN
                    case 0x04: writeEnabled = false; break;  // WRDI
                    case 0xB9: break;                       // SLEEP
                    case 0x9F: case 0x05: case 0x01:
                    case 0x03: case 0x0B: case 0x02: break;
                    default: this.Log(LogLevel.Warning, "FM25V02A: unknown command 0x{0:X2}", data); break;
                }
                return 0xFF;
            }
            switch(command)
            {
                case 0x9F: // RDID: 6x continuation 0x7F, then 0xC2, 0x22, 0x08
                    var n = idx - 1;
                    if(n < 6) { return 0x7F; }
                    if(n == 6) { return 0xC2; }
                    if(n == 7) { return 0x22; }
                    if(n == 8) { return 0x08; }
                    return 0x00;
                case 0x05: // RDSR
                    return (byte)(status | (writeEnabled ? 0x02 : 0x00));
                case 0x01: // WRSR
                    if(idx == 1) { status = (byte)(data & 0x8C); }
                    return 0xFF;
                case 0x03: case 0x0B: case 0x02:
                    if(idx == 1) { address = (data << 8); return 0xFF; }
                    if(idx == 2)
                    {
                        address = (address | data) & (Size - 1);
                        return 0xFF;
                    }
                    if(command == 0x0B && idx == 3) { return 0xFF; } // dummy byte
                    if(command == 0x02)
                    {
                        if(writeEnabled) { memory[address] = data; }
                        address = (address + 1) & (Size - 1);
                        return 0xFF;
                    }
                    var v = memory[address];
                    address = (address + 1) & (Size - 1);
                    return v;
            }
            return 0xFF;
        }

        private void EndFrame()
        {
            if(index > 0 && command == 0x02) { writeEnabled = false; } // WEL clears after WRITE
            index = 0;
            command = 0;
        }

        private const int Size = 32 * 1024;
        private readonly byte[] memory;
        private bool selected;
        private bool writeEnabled;
        private int index;
        private byte command;
        private int address;
        private byte status;
    }
}
```

</details>

<details><summary>renode/I2cWidthAdapter.cs (the I2C width adapter)</summary>

```csharp
// phase-547 W2 -- Renode's STM32F7_I2C (sealed) accepts 32-bit accesses only and
// drops 16-bit ones; NuttX's stm32h7 I2C driver reads ISR/RXDR and writes TXDR
// 16 bits wide. This adapter sits at the real I2C address and forwards every
// access, widened to 32 bits, to the stock model moved to a spare address.
using System;
using Antmicro.Renode.Core;
using Antmicro.Renode.Peripherals.Bus;

namespace Antmicro.Renode.Peripherals.Miscellaneous
{
    public class I2cWidthAdapter : IDoubleWordPeripheral, IWordPeripheral, IBytePeripheral, IKnownSize
    {
        public I2cWidthAdapter(IMachine machine, ulong target)
        {
            this.machine = machine;
            this.target = target;
        }

        public long Size => 0x400;

        public void Reset() { }

        public uint ReadDoubleWord(long offset) => machine.SystemBus.ReadDoubleWord(target + (ulong)offset);
        public void WriteDoubleWord(long offset, uint value) => machine.SystemBus.WriteDoubleWord(target + (ulong)offset, value);

        public ushort ReadWord(long offset)
        {
            var aligned = offset & ~3L;
            var v = ReadDoubleWord(aligned);
            return (ushort)(v >> (int)((offset - aligned) * 8));
        }

        public void WriteWord(long offset, ushort value)
        {
            // Every register NuttX writes 16-bit (TXDR) has nothing in its upper half.
            WriteDoubleWord(offset & ~3L, value);
        }

        public byte ReadByte(long offset)
        {
            var aligned = offset & ~3L;
            return (byte)(ReadDoubleWord(aligned) >> (int)((offset - aligned) * 8));
        }

        public void WriteByte(long offset, byte value) => WriteDoubleWord(offset & ~3L, value);

        private readonly IMachine machine;
        private readonly ulong target;
    }
}
```

</details>

`px4-scratch-patches.diff` (below with one line of context) is applied by
`20-px4-scratch.sh`. It turns off the four DMA-backed UART directions above, adds
the `KEEP` line, starts the module from `rc.board_sensors`, and adds two board
labels: `nros` (room in flash) and `wqstk` (`nros` plus the issue 2398 queue
remedy). The second label is not called `nrosstk`: PX4 matched
`px4_fmu-v6x_nrosstk` to the `nros` label, silently.

<details><summary>px4-scratch-patches.diff</summary>

```diff
diff --git a/boards/px4/fmu-v6x/init/rc.board_sensors b/boards/px4/fmu-v6x/init/rc.board_sensors
index 75eaba3d..5a9ffab5 100644
--- a/boards/px4/fmu-v6x/init/rc.board_sensors
+++ b/boards/px4/fmu-v6x/init/rc.board_sensors
@@ -153 +153,4 @@ unset INA_CONFIGURED
 unset HAVE_PM2
+
+# phase-547 spike: start the nano-ros uORB demo (typed `start` at nsh> was not seen)
+nros_uorb_demo start
diff --git a/boards/px4/fmu-v6x/nros.px4board b/boards/px4/fmu-v6x/nros.px4board
new file mode 100644
index 00000000..3817303f
--- /dev/null
+++ b/boards/px4/fmu-v6x/nros.px4board
@@ -0,0 +1,6 @@
+# phase-547 spike: room in the 1920 KiB flash for nano-ros (none of these can run in the emulator)
+# CONFIG_BOARD_UAVCAN_TIMER_OVERRIDE is not set
+CONFIG_DRIVERS_UAVCAN=n
+CONFIG_DRIVERS_IMU_INVENSENSE_ICM42688P=n
+CONFIG_DRIVERS_GPS=n
+CONFIG_MODULES_TEMPERATURE_COMPENSATION=n
diff --git a/boards/px4/fmu-v6x/nuttx-config/nsh/defconfig b/boards/px4/fmu-v6x/nuttx-config/nsh/defconfig
index a99d28eb..07b176bb 100644
--- a/boards/px4/fmu-v6x/nuttx-config/nsh/defconfig
+++ b/boards/px4/fmu-v6x/nuttx-config/nsh/defconfig
@@ -297,5 +297,3 @@ CONFIG_UART5_IFLOWCONTROL=y
 CONFIG_UART5_OFLOWCONTROL=y
-CONFIG_UART5_RXDMA=y
 CONFIG_UART5_TXBUFSIZE=10000
-CONFIG_UART5_TXDMA=y
 CONFIG_UART7_BAUD=57600
@@ -304,5 +302,3 @@ CONFIG_UART7_OFLOWCONTROL=y
 CONFIG_UART7_RXBUFSIZE=600
-CONFIG_UART7_RXDMA=y
 CONFIG_UART7_TXBUFSIZE=3000
-CONFIG_UART7_TXDMA=y
 CONFIG_UART8_BAUD=57600
@@ -320,6 +316,4 @@ CONFIG_USART3_BAUD=57600
 CONFIG_USART3_RXBUFSIZE=180
-CONFIG_USART3_RXDMA=y
 CONFIG_USART3_SERIAL_CONSOLE=y
 CONFIG_USART3_TXBUFSIZE=1500
-CONFIG_USART3_TXDMA=y
 CONFIG_USART6_BAUD=57600
diff --git a/boards/px4/fmu-v6x/nuttx-config/scripts/script.ld b/boards/px4/fmu-v6x/nuttx-config/scripts/script.ld
index b6b341d4..0895fafa 100644
--- a/boards/px4/fmu-v6x/nuttx-config/scripts/script.ld
+++ b/boards/px4/fmu-v6x/nuttx-config/scripts/script.ld
@@ -152,2 +152,4 @@ SECTIONS
 		*(.rodata .rodata.*)
+		/* phase-547 spike: nano-ros bakes NROS_BOOT_CONFIG into an orphan section */
+		KEEP(*(.nros_boot_config))
 		*(.gnu.linkonce.t.*)
diff --git a/boards/px4/fmu-v6x/wqstk.px4board b/boards/px4/fmu-v6x/wqstk.px4board
new file mode 100644
index 00000000..a8b0fbe4
--- /dev/null
+++ b/boards/px4/fmu-v6x/wqstk.px4board
@@ -0,0 +1,7 @@
+# phase-547 spike, experiment 1 (a name no other label prefixes): nros.px4board plus the issue 2398 work-queue remedy
+# CONFIG_BOARD_UAVCAN_TIMER_OVERRIDE is not set
+CONFIG_DRIVERS_UAVCAN=n
+CONFIG_DRIVERS_IMU_INVENSENSE_ICM42688P=n
+CONFIG_DRIVERS_GPS=n
+CONFIG_MODULES_TEMPERATURE_COMPENSATION=n
+CONFIG_WQ_RATE_CTRL_STACKSIZE=8192
```

</details>

`nros-spike-patches.diff` holds the W3 table's nano-ros-side measures: the timer
stubs, the `nros_platform_panic` stub, the freestanding C++ measures
(`__atomic` builtins, the riscv64-threadx `cxx-compat` shim searched with
`-idirafter`, `hrt_absolute_time()`), the `link` rename, the cast-align cast,
`CXX_STANDARD 17`, and an `NROS_SPIKE_STACK_MAIN` input for the experiments.
Oz, fat LTO and one codegen unit are `CARGO_PROFILE_RELEASE_*` variables in
`31-nros-cross.sh`, not a patch. Below with zero context, which needs
`git apply --unidiff-zero`; the saved file has the usual three lines.

<details><summary>nros-spike-patches.diff</summary>

```diff
diff --git a/examples/px4/cpp/firmware/src/modules/nros_uorb_demo/CMakeLists.txt b/examples/px4/cpp/firmware/src/modules/nros_uorb_demo/CMakeLists.txt
index b8fed26911..cea8d38ad8 100644
--- a/examples/px4/cpp/firmware/src/modules/nros_uorb_demo/CMakeLists.txt
+++ b/examples/px4/cpp/firmware/src/modules/nros_uorb_demo/CMakeLists.txt
@@ -54,0 +55,6 @@ set(_demo_ws "${CMAKE_CURRENT_LIST_DIR}/../..")
+# phase-547 spike: the main task's stack is an experiment input (unset = PX4's
+# 2048 B STACK_MAIN default, which is what the spike ran).
+set(_spike_stack)
+if(DEFINED NROS_SPIKE_STACK_MAIN AND NOT NROS_SPIKE_STACK_MAIN STREQUAL "")
+	set(_spike_stack STACK_MAIN ${NROS_SPIKE_STACK_MAIN})
+endif()
@@ -62,0 +69 @@ nros_px4_add_module(
+	${_spike_stack}
@@ -63,0 +71,8 @@ nros_px4_add_module(
+# phase-547 spike: PX4 builds modules as gnu++14; nano-ros headers need C++17
+# (inline variables). A -std= flag loses to the property, so set the property.
+set_target_properties(modules__nros_uorb_demo PROPERTIES CXX_STANDARD 17)
+# phase-547 spike: PX4 compiles C++ -nostdinc++ against NuttX's C-only include/cxx,
+# which has no <type_traits>/<new>/<utility>. Borrow the riscv64-threadx
+# freestanding shim, searched AFTER every real include dir so it only fills gaps.
+target_compile_options(modules__nros_uorb_demo PRIVATE
+	-idirafter ${NROS_REPO_DIR}/packages/boards/nros-board-threadx-qemu-riscv64/cxx-compat)
diff --git a/integrations/px4/include/nros_px4/sched_task_work_item.hpp b/integrations/px4/include/nros_px4/sched_task_work_item.hpp
index c279a56e4a..950fe3a899 100644
--- a/integrations/px4/include/nros_px4/sched_task_work_item.hpp
+++ b/integrations/px4/include/nros_px4/sched_task_work_item.hpp
@@ -48 +48,2 @@
-#include <chrono>
+// phase-547 spike: no <chrono> under PX4 NuttX's -nostdinc++; PX4's own clock
+#include <drivers/drv_hrt.h>
@@ -339,3 +340 @@ private:
-		return static_cast<uint64_t>(std::chrono::duration_cast<std::chrono::nanoseconds>(
-						     std::chrono::steady_clock::now().time_since_epoch())
-						     .count());
+		return static_cast<uint64_t>(hrt_absolute_time()) * 1000u;
diff --git a/packages/api/nros-cpp/include/rclcpp/future.hpp b/packages/api/nros-cpp/include/rclcpp/future.hpp
index d2b49833c4..ec308f6a95 100644
--- a/packages/api/nros-cpp/include/rclcpp/future.hpp
+++ b/packages/api/nros-cpp/include/rclcpp/future.hpp
@@ -151 +151,2 @@ struct PendingList {
-    void link(PendingLink* l) {
+    // phase-547 spike: NuttX unistd.h defines a function-like macro link(p1, p2)
+    void link_front(PendingLink* l) {
@@ -570 +571 @@ template <typename T, size_t Cap = ::rclcpp::rx_buffer_capacity<T>::value> class
-        if (list != nullptr) list->link(&link_);
+        if (list != nullptr) list->link_front(&link_);
@@ -587 +588 @@ template <typename T, size_t Cap = ::rclcpp::rx_buffer_capacity<T>::value> class
-        if (list != nullptr) list->link(&link_);
+        if (list != nullptr) list->link_front(&link_);
diff --git a/packages/platform/nros-platform-posix/src/px4_spike_stubs.c b/packages/platform/nros-platform-posix/src/px4_spike_stubs.c
new file mode 100644
index 0000000000..0f935ccc02
--- /dev/null
+++ b/packages/platform/nros-platform-posix/src/px4_spike_stubs.c
@@ -0,0 +1,13 @@
+/*
+ * phase-547 spike measure: no board provides nros_platform_panic on PX4.
+ * Fix it implies: the PX4 integration provides it (PX4_PANIC / board reset).
+ */
+#include <nros/platform.h>
+
+#include <stdio.h>
+#include <stdlib.h>
+
+void nros_platform_panic(const char *msg, size_t len) {
+    printf("nros: panic: %.*s\n", (int) len, msg);
+    abort();
+}
diff --git a/packages/platform/nros-platform-posix/src/timer.c b/packages/platform/nros-platform-posix/src/timer.c
index aa365874a3..defd85d95b 100644
--- a/packages/platform/nros-platform-posix/src/timer.c
+++ b/packages/platform/nros-platform-posix/src/timer.c
@@ -20,0 +21,20 @@
+#if defined(__NuttX__) && !defined(CONFIG_SIG_EVTHREAD)
+/* phase-547 spike measure: PX4's fmu-v6x NuttX has no CONFIG_SIG_EVTHREAD, so
+ * SIGEV_THREAD does not exist. Stub the timer ABI (every call fails cleanly).
+ * Fix it implies: a work-queue timer backend for PX4, or a required fragment. */
+void *nros_platform_timer_create_periodic(uint32_t period_us,
+                                          nros_platform_timer_callback_t callback,
+                                          void *user_data) {
+    (void) period_us; (void) callback; (void) user_data;
+    return 0;
+}
+void *nros_platform_timer_create_oneshot(uint32_t timeout_us,
+                                         nros_platform_timer_callback_t callback,
+                                         void *user_data) {
+    (void) timeout_us; (void) callback; (void) user_data;
+    return 0;
+}
+void nros_platform_timer_destroy(void *handle) { (void) handle; }
+int8_t nros_platform_timer_cancel(void *handle) { (void) handle; return -1; }
+#else
+
@@ -133,0 +154 @@ int8_t nros_platform_timer_cancel(void *handle) {
+#endif /* SIGEV_THREAD available */
diff --git a/packages/rmw/uorb/nros-rmw-uorb/src/px4_callback_glue.cpp b/packages/rmw/uorb/nros-rmw-uorb/src/px4_callback_glue.cpp
index 5c447594a3..4d9ed0904a 100644
--- a/packages/rmw/uorb/nros-rmw-uorb/src/px4_callback_glue.cpp
+++ b/packages/rmw/uorb/nros-rmw-uorb/src/px4_callback_glue.cpp
@@ -113 +113 @@ struct Slot {
-        return reinterpret_cast<CallbackSlotImpl *>(storage);
+        return static_cast<CallbackSlotImpl *>(static_cast<void *>(storage)); // phase-547 spike: -Wcast-align
diff --git a/packages/rmw/uorb/nros-rmw-uorb/src/session.cpp b/packages/rmw/uorb/nros-rmw-uorb/src/session.cpp
index 92173f7343..c2aa8cdf43 100644
--- a/packages/rmw/uorb/nros-rmw-uorb/src/session.cpp
+++ b/packages/rmw/uorb/nros-rmw-uorb/src/session.cpp
@@ -24 +24 @@
-#include <atomic>
+// phase-547 spike: PX4 NuttX C++ is -nostdinc++ (no <atomic>); __atomic builtins instead
@@ -50,2 +50,2 @@ struct SessionState {
-    std::atomic<void (*)(void *)> wake_cb{nullptr};
-    std::atomic<void *> wake_ctx{nullptr};
+    void (*wake_cb)(void *){nullptr};
+    void * wake_ctx{nullptr};
@@ -122,3 +122,3 @@ rmw_ret_t session_set_wake_callback(rmw_session_t *session, void (*cb)(void *ctx
-    state->wake_cb.store(nullptr, std::memory_order_release);
-    state->wake_ctx.store(ctx, std::memory_order_release);
-    state->wake_cb.store(cb, std::memory_order_release);
+    __atomic_store_n(&state->wake_cb, nullptr, __ATOMIC_RELEASE);
+    __atomic_store_n(&state->wake_ctx, ctx, __ATOMIC_RELEASE);
+    __atomic_store_n(&state->wake_cb, cb, __ATOMIC_RELEASE);
@@ -133 +133 @@ void session_wake(void *session_backend_data) {
-    void (*cb)(void *) = state->wake_cb.load(std::memory_order_acquire);
+    void (*cb)(void *) = __atomic_load_n(&state->wake_cb, __ATOMIC_ACQUIRE);
@@ -135 +135 @@ void session_wake(void *session_backend_data) {
-        cb(state->wake_ctx.load(std::memory_order_acquire));
+        cb(__atomic_load_n(&state->wake_ctx, __ATOMIC_ACQUIRE));
diff --git a/packages/rmw/uorb/nros-rmw-uorb/src/subscriber.cpp b/packages/rmw/uorb/nros-rmw-uorb/src/subscriber.cpp
index 4c1c1231e1..2413c440a3 100644
--- a/packages/rmw/uorb/nros-rmw-uorb/src/subscriber.cpp
+++ b/packages/rmw/uorb/nros-rmw-uorb/src/subscriber.cpp
@@ -40 +40 @@
-#include <atomic>
+// phase-547 spike: PX4 NuttX C++ is -nostdinc++ (no <atomic>); __atomic builtins instead
@@ -56 +56 @@ struct SubscriberState {
-    std::atomic<bool> ready;
+    bool ready;
@@ -67,2 +67,2 @@ struct SubscriberState {
-    std::atomic<rmw_event_callback_t> on_new;
-    std::atomic<const void*> on_new_ud;
+    rmw_event_callback_t on_new;
+    const void* on_new_ud;
@@ -73,2 +73,2 @@ extern "C" void subscriber_ready_callback(void* arg) {
-    state->ready.store(true, std::memory_order_release);
-    rmw_event_callback_t cb = state->on_new.load(std::memory_order_acquire);
+    __atomic_store_n(&state->ready, true, __ATOMIC_RELEASE);
+    rmw_event_callback_t cb = __atomic_load_n(&state->on_new, __ATOMIC_ACQUIRE);
@@ -76 +76 @@ extern "C" void subscriber_ready_callback(void* arg) {
-        cb(state->on_new_ud.load(std::memory_order_acquire), 1);
+        cb(__atomic_load_n(&state->on_new_ud, __ATOMIC_ACQUIRE), 1);
@@ -131 +131 @@ subscription_create(const rmw_node_t* node, const rmw_message_type_support_t* /*
-        state->ready.store(true, std::memory_order_relaxed);
+        __atomic_store_n(&state->ready, true, __ATOMIC_RELAXED);
@@ -146 +146 @@ subscription_create(const rmw_node_t* node, const rmw_message_type_support_t* /*
-        state->ready.store(true, std::memory_order_relaxed);
+        __atomic_store_n(&state->ready, true, __ATOMIC_RELAXED);
@@ -165,3 +165,3 @@ rmw_ret_t subscription_set_on_new_message_callback(rmw_subscription_t* subscribe
-    state->on_new.store(nullptr, std::memory_order_release);
-    state->on_new_ud.store(user_data, std::memory_order_release);
-    state->on_new.store(callback, std::memory_order_release);
+    __atomic_store_n(&state->on_new, nullptr, __ATOMIC_RELEASE);
+    __atomic_store_n(&state->on_new_ud, user_data, __ATOMIC_RELEASE);
+    __atomic_store_n(&state->on_new, callback, __ATOMIC_RELEASE);
@@ -221 +221 @@ rmw_ret_t subscription_take(const rmw_subscription_t* subscriber, rmw_mut_byte_s
-    if (state->callback_active && !state->ready.load(std::memory_order_acquire)) {
+    if (state->callback_active && !__atomic_load_n(&state->ready, __ATOMIC_ACQUIRE)) {
@@ -233 +233 @@ rmw_ret_t subscription_take(const rmw_subscription_t* subscriber, rmw_mut_byte_s
-            state->ready.store(false, std::memory_order_release);
+            __atomic_store_n(&state->ready, false, __ATOMIC_RELEASE);
@@ -251 +251 @@ rmw_ret_t subscription_take(const rmw_subscription_t* subscriber, rmw_mut_byte_s
-        state->ready.store(false, std::memory_order_release);
+        __atomic_store_n(&state->ready, false, __ATOMIC_RELEASE);
@@ -269 +269 @@ rmw_ret_t subscription_has_data(rmw_subscription_t* subscriber, bool* out_has_da
-    if (state->callback_active && !state->ready.load(std::memory_order_acquire)) {
+    if (state->callback_active && !__atomic_load_n(&state->ready, __ATOMIC_ACQUIRE)) {
```

</details>

#### Results (2026-11-10)

| image | `STACK_MAIN` | `wq:rate_ctrl` | result |
|---|---|---|---|
| `build-nros` (label `nros`) | 2048 B (PX4 default) | 3150 B | **the original fault, reproduced exactly**: MemManage in `mm_free` right after `nros: session open (rmw=uorb)`, task `netinit`, `MMFAR ffffff94`, `R3 ffffff90`, from `netlib_getmacaddr` closing a socket in `netinit_dhcp`; then a fault-reboot loop |
| `build-nros16k` (label `nros`) | 16384 B | 3150 B | `mm_free` fault gone; a HardFault (`INVSTATE`) in task `ekf2`, its stack pointer outside its own stack, just after the module's first publish |
| `build-wqstk` (label `wqstk`) | 16384 B | 8192 B | **no fault** in 70 s of virtual time; the module publishes every second, and PX4's `listener debug_key_value` prints `key: "nros"`, `value: 7.00000`. The subscription half received nothing until [issue 2403](../issues/archived/2403-px4-uorb-subscription-never-taken.md)'s fix (image `build-fix2403`: `recv vehicle_status`, `status=51` at 30 ticks) |

So the `mm_free` fault was the main task's stack, as [issue 2398](../issues/archived/2398-px4-thread-stacks-below-executor-floor.md)
predicted (now confirmed there), and the queue's stack is a second, separate
overflow. Both are the values 2398's configure-time refusal requires. The
guarded-heap build (step 4 of the plan) was not needed. [Issue 2397](../issues/archived/2397-px4-nuttx-platform-type-sizes.md)
is resolved with this.

W3's round trip works both ways. nano-ros to PX4 works, read by a stock PX4
tool. PX4 to nano-ros did not at first: commander publishes `vehicle_status`
(PX4's `listener` shows `nav_state: 4`) and the uORB broker called our
callback, but the subscription's NuttX fd belonged to the `start` task's group,
so every `orb_check` on the work-queue thread was EBADF (issue 2403). With the
subscription held by pointer the demo prints `recv vehicle_status:
nav_state=4 arming_state=1` and counts `status=51` at 30 ticks.

### Where this leaves the board decision

The emulator side holds up: stock Renode plus small models runs stock PX4 for
the 6X to `nsh>`, and runs nano-ros's module far enough to open a uORB session.
Every stop since has been a nano-ros portability defect that a real Pixhawk
would hit identically, which is what the twin is for. The open question has
moved from "can it be emulated" to "how much nano-ros work makes PX4-on-NuttX a
supported target", and the table above is that list.
