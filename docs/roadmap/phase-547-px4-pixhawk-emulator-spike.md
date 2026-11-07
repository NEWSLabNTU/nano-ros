# Phase 547 — Spike: does PX4's NuttX firmware for a shippable Pixhawk boot in an emulator?

**Status (2026-11-07). OPEN — W1 in progress.** A time-boxed spike (about
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

(Filled in as work items complete.)
