---
rfc: 0115
title: "Emulator-first board policy: a board enters the book with an emulator twin that runs the same ELF"
status: Draft
since: 2026-10
last-reviewed: 2026-10
implements-tracked-by: [phase-524]
amends: [RFC-0064]
supersedes: []
superseded-by: null
---

# RFC-0115 — Emulator-first board policy

**Status:** Draft (2026-10-31). Written from the adoption discussion of
2026-10-31, before any spike has run. Implemented by
[phase-524](../roadmap/phase-524-emulator-twins.md). Companion RFCs from the
same discussion: RFC-0114 (the board support registry as the single primitive,
which this RFC's evidence page reads from) and RFC-0116 (adoption purposes and
front doors, which decides serial-first chapters and the `nros run` verb). This
RFC does not restate either; it names what it needs from them.

## Summary

nano-ros is adopted for three purposes: **industrial** (safety and robustness
are hard requirements), **field solutions** (technicians building per-customer
systems with entry-level programming), and **education** (college and
engineering-school labs). All three hold a physical board, none of them holds
QEMU. Today every in-tree board a chapter boots is a QEMU machine or a host
simulator, and the chapters' flash lines say so: no probe runs them.

This RFC decides three things.

- **D3 — Family board packages, configured by one field.** nano-ros ships one
  board package per RTOS family, configured for a specific board by one field
  (`west_board` for Zephyr, an overlay for FreeRTOS), plus one worked
  configuration per board the book names. It does not ship a package for every
  board. The chapter that teaches the one field, "Name your board", lives in
  getting-started, not in porting.
- **D4 — Emulator twin required.** A board enters the book only when an
  emulator boots the same ELF the bench board runs. The emulator series is:
  QEMU `mps2-an385` for concepts (the gate emulator, with no hardware twin by
  design), Renode for Nucleo-F767ZI and Arduino UNO R4 Minima, Wokwi for
  Raspberry Pi Pico W, and the Arm FVP for industrial use, maintainer-run.
- **D8 — One evidence page per board**, generated from the registry RFC-0114
  defines: what runs it, how often, what breadth it inherits, when hardware
  last booted it.

The board set is Nucleo-F767ZI, Arduino UNO R4 Minima and Raspberry Pi Pico W,
all three on Zephyr, all three reachable over USB serial first. The NXP S32K344
stays the industrial board, hardware-only. Arduino GIGA R1 WiFi, Nucleo-H743ZI
and an ESP32 revival are dropped for now, each for a stated reason.

## Problem

1. **Nobody holds QEMU.** The book's platform chapters boot `mps2-an385`,
   `qemu-armv7a`, `rv-virt`, `native_sim` and a Linux simulator. The weekly
   `probe-platforms.yml` tracks prove those lines run. A technician, a student
   or an industrial integrator has a board on the desk, and the chapter's flash
   line for it is the one line no probe runs (issue 2176 made every such line
   say so).
2. **Board count is not the gap; the front door is.** Zephyr 3.7 in our
   workspace already knows every Nucleo and Disco, the Arduino `giga_r1`,
   `portenta_h7`, `nano_33_ble`, `uno_r4`, `opta`, the `rpi_pico` and more.
   Naming one is a one-field edit of a Zephyr board descriptor. That knowledge
   sits under `book/src/porting/`, the expert shelf, and no getting-started
   chapter shows "same application, change `west_board`, flash".
3. **Hardware lanes do not exist and will not soon.** A CI runner has no board
   (issue 2176). So the only way a chapter's line can be replayed before a
   release is an emulator that runs the SAME ELF the bench runs. QEMU emulates
   none of the boards people buy; Renode and Wokwi emulate several.
4. **Test effort.** Every board that joins the tree costs matrix cells, fixture
   rows, probe tracks and book sections if it is treated like a witness. Three
   boards treated that way triple the Zephyr cell count for no new coverage,
   since they share the family's code path.

## Design

### D3 — Family board packages, configured by one field

- A board package is per RTOS family: `nros-board-zephyr` (descriptor,
  `west_board = "<zephyr id>"`, network from Zephyr's own configuration under
  RFC-0109 `stack = "os"`), `nros-board-freertos` (family driver, a board is an
  overlay that re-exports its `Config`), and the NuttX and ThreadX equivalents.
  This is what RFC-0064 already organises; this decision makes it the policy
  for every board the book names.
- For each board the book names, the tree carries ONE worked configuration: a
  descriptor directory under `packages/boards/` with `nros-board.toml`,
  `package.xml`, and the Kconfig fragment or overlay the board needs, and a
  registry row (RFC-0114). Nothing else per board.
- A board the book does not name is the reader's to configure: the
  getting-started chapter "Name your board" shows the one field, the three
  lines of `nros-board.toml` around it, and `nros build`. The porting chapters
  keep the deep road (a board crate, a new platform) and are linked from the
  end of that chapter, not the start.
- The policy does not claim support for a board the reader names. Support is
  the registry's word (RFC-0114's status), and a reader-named board has no row.

### D4 — A board enters the book only with an emulator twin

- **Twin** means: the same ELF `nros build` produces for the bench board boots
  in the emulator, reaches a router on the host, and prints the chapter's
  expected output. Not a "similar" machine, not a different board id.
- **Series.**

  | Emulator | Boards | Role |
  |---|---|---|
  | QEMU `mps2-an385` | bare-metal, FreeRTOS and Zephyr witnesses | Concepts; the gate emulator for the family matrix; deliberately no hardware twin (an MPS2 board is not what adopters buy) |
  | Renode | Nucleo-F767ZI, Arduino UNO R4 Minima | Twin of a bench board; serial first, ethernet on the Nucleo |
  | Wokwi | Raspberry Pi Pico W | Twin of a bench board; serial first, WiFi when the Zephyr 4.x line carries the driver |
  | Arm FVP | `fvp-aemv8r-smp` | Industrial purpose, maintainer-run (issue 2176); not a book board under this rule until a track replays it |

- **Probe tracks.** Each twin gets an installed-track probe like the QEMU
  chapters have (phase-517 W10): `renode-uno-r4`, `renode-nucleo`,
  `wokwi-pico`. They are generated from the registry (RFC-0114 D12), not
  authored in the workflow matrix.
- **Hardware.** The bench run is maintainer-run and dated in the registry
  (`runs = "hardware"`, `cadence = "manual"`, `last_run`). The flash line in
  the chapter drops the 2176 "unverified" wording only when that date exists.
- **Breadth is a family property** (RFC-0114 D11). Zephyr's feature × language
  × RMW × edition matrix is asserted once, on the QEMU `mps2_an385` and
  `native_sim` witnesses. A twin board adds one smoke cell (the talker boots,
  the link comes up, one message is heard) and inherits the family breadth; the
  evidence page says so in those words. Three new Zephyr boards therefore cost
  three smoke cells and three probe tracks, not three matrices.

### D8 — One evidence page per board

Generated from the registry, per board: package, platform family, status word
(RFC-0114 D10), `runs` and the emulator or hardware it names, cadence and last
run, breadth (own or inherited and from which witness), the chapter and its
run and flash anchors, maintainers, and the open issues tagged for it. The
hand-written tier page retires with RFC-0114; this page is what the three
purposes read: industrial for qualification, education for a syllabus line,
field solutions for "supported or not".

### Board picks

Cross-checked 2026-10-31 against Zephyr 3.7's board YAML in the in-tree
workspace, Renode's `platforms/boards`, and Wokwi's supported-hardware page.

| Board | Purpose | Zephyr 3.7 IO we can use | Emulator twin | Emulated IO |
|---|---|---|---|---|
| Nucleo-F767ZI | education; industrial broad reference | UART over the ST-LINK virtual COM port, `netif:eth`, USB device | Renode `nucleo_f767zi.repl` | UART, GPIO, LEDs, button; ethernet PHY declared, MAC comes from `platforms/cpus/stm32f777.repl` (to verify by booting) |
| Arduino UNO R4 Minima | field solutions | GPIO, UART only | Renode `arduino_uno_r4_minima.repl` (via `platforms/cpus/renesas-r7fa4m1a.repl`) | UART, GPIO |
| Raspberry Pi Pico W | field solutions and education bridge | UART, GPIO, USB device, PIO. WiFi needs the Zephyr 4.x AIROC driver: 3.7's AIROC is SDIO-only and the CYW43439 sits on SPI/PIO | Wokwi RP2040 Pico W | UART, GPIO; WiFi simulated with a virtual access point and gateway (documented for the Pico 2 W) |
| NXP S32K344 | industrial | FreeRTOS, lwIP ethernet (in hand) | none claimed (Renode ships only `nxp-s32k388evb.repl`, S32K3 peripherals partial) | — |

Dropped, with the reason:

- **Arduino GIGA R1 WiFi.** No emulator anywhere (not Renode, not Wokwi, not
  QEMU). WiFi is not reachable on Zephyr 3.7: the board selects the AIROC
  CYW4343W part, the AIROC driver needs an SDIO host, and 3.7's `drivers/sdhc`
  has no STM32 host. Brand value only; revisit when Arduino Core on Zephyr 4.4
  and an emulator exist.
- **Nucleo-H743ZI.** Renode has the `nucleo_h753zi.repl` sibling, but the
  F767ZI already covers ethernet Cortex-M7 for education and industrial.
  Redundant.
- **ESP32 revival.** The bare-metal `esp-hal` road is dormant (issue 1525) and
  Zephyr esp32 is unsupported in the index. Field solutions and education both
  lean on it; defer until the first adoption probes show demand.

### Starter interface: serial first

Every picked board's vendor getting-started page is a USB cable first: the
Nucleo through its ST-LINK virtual COM port, Arduino boards through USB-C CDC,
the Pico through its USB bootloader then USB CDC. Network is the second step on
every vendor page. nano-ros serial transport on Zephyr is zenoh-pico's built-in
COBS framing with no extra crate, and every twin emulates a UART. So the first
chapter per board is serial over USB; ethernet is chapter two, on the Nucleo;
WiFi is deferred to the Zephyr 4.4 line and the chapters say so. Arduino's own
Zephyr story confirms the ordering from the other side: Arduino Core on Zephyr
1.0 (2026-09, Zephyr 4.4.1) lists GIGA R1 WiFi, Portenta H7, Nano 33 BLE and
others, and not the UNO R4. RFC-0116 D5 owns the chapter shape; this RFC owns
the finding.

### What is NOT verified

Listed here so a reader does not mistake a pick for a measurement.

- Renode's `nucleo_f767zi` passing ethernet traffic to the host: the PHY is
  declared, the MAC is inferred from the SoC file.
- Wokwi loading a Zephyr ELF: its documented firmware types for the Pico are
  `.hex`, `.uf2` and `.elf`, and its documented ecosystems are the Pico SDK and
  MicroPython.
- GIGA R1 WiFi on Zephyr 4.x.
- Renode S32K3 fidelity.
- Pico W WiFi under Zephyr 4.4 on our line.

## Evaluation and decision steps

Three spikes, in this order, cheapest first. Each is a pass or fail with a log
under `tmp/`, and each has a decision step that rewrites D4 before any board
package, chapter or track is written.

1. **Renode UNO R4 Minima, serial.** Build the Zephyr talker for
   `arduino_uno_r4_minima`, boot it in Renode with the UART bridged to a host
   pty, run `rmw_zenohd` on the host listening on that serial locator, and
   require `Publishing: 0` and a ROS 2 `ros2 topic echo` delivery. Time box one
   day. Fail: the UNO R4 leaves the book list and field solutions take the Pico
   serial path; Renode stays for the Nucleo.
2. **Renode Nucleo-F767ZI, serial then ethernet.** Same as step 1 over the
   ST-LINK UART first, then with the Renode ethernet switch bridged to a host
   TAP or user network and a `tcp/` locator. Time box two days. Fail on
   ethernet: the Nucleo keeps a serial-only chapter and the ethernet chapter
   waits for the bench; fail on serial: the Nucleo leaves the list and
   education uses QEMU `mps2-an385` only.
3. **Wokwi CLI, Pico W, serial.** `wokwi-cli` loads our Zephyr `rpi_pico` ELF
   with the UART routed to the host and prints the talker's lines. Time box
   half a day. Fail: Pico W moves to Renode if an RP2040 platform exists in the
   installed version (the supported-boards page does not list one), else drops.

Decision step after the three: the surviving boards are the book list; D4's
table is rewritten to match; phase-524 W2 onward proceeds only for survivors.
A board without a working twin never reaches a chapter.

## Alternatives rejected

- **Hardware lanes first.** No self-hosted runner has a board, and a lane with
  a board is one board; the twin rule gives every reader the same replay a
  maintainer has.
- **Per-board packages for every board.** Triples the Zephyr cell count with
  no new coverage; RFC-0064 already chose family packages.
- **WiFi-first chapters.** Not reachable on 3.7 for any pick; vendors do not
  start there either.
- **Keeping GIGA R1 for the brand.** A board with no twin cannot be replayed,
  which is the one property this RFC exists for.

## Relationship to other RFCs

- **RFC-0064** (board support organisation): amended, the family-package rule
  becomes policy for book boards and the twin requirement is added.
- **RFC-0114**: the registry fields D4 and D8 read and write (`runs`,
  `emulator`, `hardware`, `cadence`, `last_run`, breadth inheritance).
- **RFC-0116**: the three purposes, the serial-first chapter shape, the
  single-package front door and `nros run`.
- **RFC-0109**: a Zephyr board's network is Zephyr's (`stack = "os"`), which
  is why one field suffices.
- **RFC-0099**: Renode and Wokwi join `nros-sdk-index.toml` as `[tool.*]` dist
  rows with measured floors.

## Sources

- Renode supported boards: <https://renode.readthedocs.io/en/latest/introduction/supported-boards.html>
- Renode board platform files: <https://github.com/renode/renode/tree/master/platforms/boards>
- Wokwi supported hardware: <https://docs.wokwi.com/getting-started/supported-hardware>
- Wokwi Raspberry Pi Pico 2 W (WiFi simulation and gateway): <https://docs.wokwi.com/parts/board-pi-pico-2w>
- Arduino Core on Zephyr 1.0.0: <https://blog.arduino.cc/2026/09/03/arduino-core-on-zephyr-1-0-0-is-here/>
- Zephyr Pico W WiFi sample: <https://docs.zephyrproject.org/latest/samples/boards/raspberrypi/pico_w_wifi_led/README.html>
- Zephyr 3.7 board YAML and drivers read from the in-tree west workspace
  (`boards/arduino/*`, `boards/raspberrypi/rpi_pico`, `boards/st/nucleo_f767zi`,
  `drivers/wifi/infineon/Kconfig.airoc`, `drivers/sdhc`).
