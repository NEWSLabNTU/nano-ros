# Phase 524 — Emulator twins for the boards adopters hold

**Status (2026-11-01). IN PROGRESS. W1 done: one survivor, the Nucleo-F767ZI (serial and ethernet in Renode 1.17.0); the UNO R4 Minima failed (does not fit the RA4M1) and the Pico W was blocked (no `WOKWI_CLI_TOKEN`, no RP2040 in Renode). RFC-0115 D4 rewritten to match. W2 onward run for the Nucleo only. W2 done: `[tool.renode]` (Linux x86_64 and arm64; macOS has no portable archive) and `hal_stm32` on the 3.7 line; the board row that names them is W3. W0 open.** Implements
[RFC-0115](../design/0115-emulator-first-board-policy.md). Depends on
phase-522 (the board support registry as the single primitive, RFC-0114) for
the row shape W3 writes and the generator W4 uses, and on phase-523 (adoption
front doors, RFC-0116) for the serial-first chapter shape and the `nros run`
verb W4's chapters are written against.

---

## Why

The three adoption purposes (industrial, field solutions, education) each hold
a physical board; every board a chapter boots today is QEMU or a host
simulator, and every flash line says no probe runs it (issue 2176). RFC-0115
rules that a board enters the book only with an emulator twin that boots the
same ELF the bench runs. This phase buys the boards, runs the spikes that
prove or disprove each twin, installs the emulators through `nros setup`, adds
the surviving boards as registry rows and descriptors, generates their probe
tracks, and records the first hardware runs.

## Work items

Each item ends in an acceptance and a decision step. The decision step is
where a measured result changes the plan, written down before the result is
known.

### W0 — Purchase

Nucleo-F767ZI, Arduino UNO R4 Minima (not the WiFi variant: same Zephyr reach,
the ESP32-S3 coprocessor is not driven), Raspberry Pi Pico W, USB cables, one
USB-to-UART adapter per bench for a board whose console is not the data link.
Owner: the maintainer.

- **Acceptance:** boards on the bench.
- **Decision:** none; the spikes do not need the hardware.

### W1 — The three spikes

Run in the order RFC-0115's "Evaluation and decision steps" gives, each with a
pass or fail log under `tmp/` and the exact commands in the log header.

1. Renode, `arduino_uno_r4_minima`, Zephyr talker over UART bridged to a host
   pty, `rmw_zenohd` on a serial locator, `ros2 topic echo` delivery. One day.
2. Renode, `nucleo_f767zi`, the same over the ST-LINK UART, then over the
   Renode ethernet switch bridged to the host with a `tcp/` locator. Two days.
3. `wokwi-cli`, `rpi_pico`, our Zephyr ELF with the UART routed to the host.
   Half a day.

- **Acceptance:** three logs, each ending in PASS or FAIL with the decisive
  line quoted.
- **Decision step:** rewrite RFC-0115 D4's table to the survivors before W2
  starts. A fail on UNO R4 removes it from the list; a fail on Nucleo ethernet
  keeps the Nucleo serial-only and moves its ethernet chapter behind W5; a fail
  on Nucleo serial removes it; a fail on Wokwi moves the Pico W to Renode if
  the installed Renode has an RP2040 platform, else removes it. If all three
  fail, this phase closes with the record and RFC-0115 is marked Superseded by
  a hardware-first decision.

**Result (2026-11-01). Done. Survivor: Nucleo-F767ZI.** Run in the `ros2`
distrobox at `9e6bf0d4e2` with Zephyr 3.7.0, SDK 0.16.8 and the C talker
(`examples/zephyr/c/talker`, `CONFIG_NROS_TRANSPORT_SERIAL` image shape of the
`talker-serial` row). Tools, pinned: Renode 1.17.0
(`renode-1.17.0.linux-portable.tar.gz`, sha256
`4ba7c68b59e2447f188ef4b4b112fcccd2802582c460beedceb63b84e5605a5f`) and
`wokwi-cli` 0.29.1 (`wokwi-cli-linuxstatic-x64`, sha256
`3c9d6da007d008a3822ab468318037285440055132c59fd659d7306e40f14258`), both from
their GitHub releases, digests matching the published ones.

| Spike | Result | Decisive line | Log |
|---|---|---|---|
| 1. UNO R4 Minima | FAIL | ``region `FLASH' overflowed by 144744 bytes`` / ``region `RAM' overflowed by 144904 bytes`` (best of four builds; 256 KiB / 32 KiB part) | [renode-uno-r4](data/phase-524-w1-renode-uno-r4.log) |
| 2. Nucleo-F767ZI | PASS, serial and ethernet | `data: 'Hello World: 1'` from `ros2 topic echo`, over `serial/serial@40004800` and over `tcp/192.0.2.1:7447` | [renode-nucleo](data/phase-524-w1-renode-nucleo.log) |
| 3. Pico W | FAIL (blocked) | `Error: Missing WOKWI_CLI_TOKEN environment variable.`; Renode 1.17.0 has no RP2040 platform | [wokwi-pico](data/phase-524-w1-wokwi-pico.log) |

Decision applied: the UNO R4 and the Pico W leave the list, the Nucleo keeps
both chapters, and RFC-0115 D4 names the Nucleo alone. W2 adds
`[tool.renode]` only; W3 writes one row (`renode:nucleo_f767zi`); W4
generates `renode-nucleo` only. The field solutions purpose has no twin board
now, which is a maintainer call (a new candidate, or a token for a W1.3
rerun).

What the survivor needs, found by the spike:

- **Module.** `hal_stm32` (the revision zephyr v3.7.0's own `west.yml` pins, 639 MiB shallow)
  is not in `west.yml`'s allowlist; the spike fetched it by hand. W2/W3 add a
  `[zephyr_module.hal_stm32]` row with `lines = ["3.7"]` and mirror it.
- **Rust targets.** `examples/zephyr/rust-toolchain.toml` (nightly-2026-04-11)
  lists no `thumbv7em-none-eabi(hf)`; the spike added them by `rustup target
  add`. Any Cortex-M4/M7 Zephyr board needs them there.
- **Kconfig (serial).** `CONFIG_NROS_TRANSPORT_SERIAL=y`,
  `CONFIG_NETWORKING=n`, `CONFIG_RING_BUFFER=y`,
  `CONFIG_TEST_RANDOM_GENERATOR=y`, `CONFIG_UART_INTERRUPT_DRIVEN=y`,
  `CONFIG_MAIN_THREAD_PRIORITY=5`; the zenoh link takes USART3 (the ST-LINK
  virtual COM port), so the console moves to USART6 (Arduino D0/D1) by
  overlay, and on the bench that console needs a USB-to-UART adapter.
- **Kconfig (ethernet).** Static IPv4 (`NET_CONFIG_SETTINGS`, address, mask,
  gateway); `eth_stm32_hal` comes from the board's `&mac`, entropy from
  `STM32_RNG`.
- **RAM.** sram0 is 384 KiB and the crate pool defaults do not fit it: the
  serial image needed `ZENOH_MAX_LARGE_SUBSCRIBERS = 0` and a 16 KiB kernel
  heap (89.5 % used), the ethernet image also `ZENOH_MAX_SUBSCRIBERS = 1`,
  `ZENOH_SERVICE_BUFFER_SIZE = 1024`, `ZEPHYR_HEAP_SIZE = 32768` (98.2 %
  used). These belong in the descriptor's `[knobs]`. The 32 KiB heap is below
  the C talker's measured 15,152-byte peak plus the 24,576-byte headroom
  floor (39,728), so W3 measures the boot report before settling it, or moves
  stacks or pools to the 128 KiB DTCM.
- **Twin harness.** Renode writes `~/.config/renode` even with `--config`
  (command history and a binaries cache), so a track sets `HOME` for it. The
  ethernet twin needs a tap: no root is required inside `unshare -rn`, which
  must then hold the router and `ros2` too.

### W2 — Renode and Wokwi as `[tool.*]` entries

Add `[tool.renode]` and `[tool.wokwi-cli]` to `nros-sdk-index.toml` as RFC-0099
dist rows: pinned version, per-host URL and sha256, a measured `floor`
(`scripts/sdk/measure-dist-floor.py`), `smoke` rows the release job runs. The
surviving boards' `[board.*]` rows name them in `packages`, so
`nros setup <board>` installs the twin with the toolchain. `nros doctor` reports
them. Nothing is installed by hand or by apt; the probe container is the
measurement of that.

- **Acceptance:** `nros setup <board> --dry-run` prices the emulator;
  `check-dist-floors` green; a pristine container reaches a running emulator
  from the chapter's setup block alone.
- **Decision step:** if a tool has no redistributable archive for a host we
  claim (Wokwi CLI's licence or Renode's portable build), the row carries
  `floor` and dist for the hosts it has and the chapter says which hosts the
  twin runs on; a twin that runs on no CI host removes its board from the list.

**Result (2026-11-01). Done for the Nucleo-F767ZI; the board link waits on
W3.** W1 left one survivor, so W2 adds `[tool.renode]` only. There is no
`[tool.wokwi-cli]`: the Pico W left the list, and a row for a twin no board
uses is a pin nobody exercises.

- **`[tool.renode]`**, version `1.17.0`, upstream's own portable tarballs (the
  `[tool.arm-fvp]` / `[tool.ninja]` shape: we apply nothing, so no repack).
  `subdir = "renode_{version}-portable"` (the program stays beside its bundled
  .NET runtime), `front = ["renode"]` links it into `$NROS_HOME/bin`, and
  `smoke` runs `renode --version` and expects `Renode v1.17.0`.

  | host | asset | sha256 | floor (measured off the artifact) |
  |---|---|---|---|
  | linux-x86_64 | `renode-1.17.0.linux-portable.tar.gz` | `4ba7c68b...a5f` | glibc 2.29, glibcxx 3.4.26 |
  | linux-arm64 | `renode-1.17.0.linux-arm64-portable.tar.gz` | `e9051578...f1f` | glibc 2.34, glibcxx 3.4.26 |
  | macos-arm64 | none | | |

  Decision step applied: macOS has no portable archive upstream, only
  `renode-1.17.0.osx-arm64-portable.dmg`, a disk image no dist `install` here
  opens and nothing in this tree can measure or smoke. So the row carries the
  two Linux hosts, `nros setup` refuses on a mac with its no-dist message, and
  W4's chapter says the twin runs on Linux. Both CI hosts are Linux x86_64, so
  the board stays on the list.
- **`[zephyr_module.hal_stm32]`** moves from `lines = []` to
  `lines = ["3.7"]`, `needed_by = ["nucleo_f767zi"]`, `approx_mb = 639` (the
  spike's shallow checkout), and `west.yml`'s allowlist names it. Its revision
  is not stated anywhere: `west.yml` imports it from zephyr v3.7.0, so it is
  v3.7.0's own pin, `f1317150eac951fdd8259337a47cbbc4c2e6d335`.
- **Measured.** `nros setup --tool renode --dry-run` plans `prebuilt 1.17.0
  (dist linux-x86_64)`; `nros setup zephyr --dry-run` prices `hal_stm32 639 MB
  [3.7] nucleo_f767zi`. A real `nros setup --tool renode` into the store, then
  `renode --version` from `$NROS_HOME/bin` with an empty `HOME`, prints
  `Renode v1.17.0` and leaves that `HOME` empty. `check-dist-floors`,
  `check-smoke-or-reason`, `check-dist-or-reason` and
  `check-zephyr-module-allowlist` green. `check-dist-runtime-deps` over a store
  holding renode finds nothing undeclared for it: the program needs only the
  loader family and libstdc++, and the 11 GTK/X11 sonames belong to
  `renode-ui`, the GUI monitor a headless twin never opens (an upstream note).
- **Left for W3.** The board's `[board.*]` row, which names `renode` (and the
  Zephyr toolchain) in `packages` so `nros setup <board> --dry-run` prices the
  twin, does not exist yet: it is W3's, blocked on phase-522 W1. Until then the
  acceptance is met per tool (`--tool renode`), not per board. Also W3's: the
  `thumbv7em-none-eabi(hf)` Rust targets in `examples/zephyr/rust-toolchain.toml`.
  The pristine-container acceptance is W4's probe track.

### W3 — Registry rows and board descriptors

After phase-522 W1 has landed the new registry fields. One row per surviving
board: `package`, `platform = "zephyr"`, `runs = "emulator"`, `emulator`
(`renode:nucleo_f767zi`, `renode:arduino_uno_r4_minima`, `wokwi:pi-pico-w`),
`hardware` (the bench board id), `cadence = "weekly"`, `breadth = "inherits
ZephyrQemuCortexM"`, the chapter and its anchors, `maintainers`. One descriptor
directory per board under `packages/boards/` with `nros-board.toml`
(`west_board`, `rust_target`, serial console node, RFC-0109 `stack = "os"`),
`package.xml`, and the Kconfig fragment the board needs (UART interrupt-driven
RX for the serial link, issue 0852). No crate.

- **Acceptance:** `check-board-registry` green; `nros board list` shows the
  three with status "Verified in emulator (weekly)" once W4's track has run,
  "Builds" before; the generated evidence page says breadth is inherited and
  names the witness.
- **Decision step:** if phase-522 W1 is not on `main` when W2 finishes, W3
  writes rows in the OLD schema (`tier = 3`, `execution_class = "cross-run"`)
  and the phase-522 migration carries them; it does not wait.

### W4 — Probe tracks and serial-first chapters

Generate the tracks `renode-uno-r4`, `renode-nucleo`, `wokwi-pico` from the
registry (RFC-0114 D12): the `probe-platforms.yml` matrix rows, the
`run-bootstrap-probe.sh` lookup, and the issue 2169 run-command rows. Write one
getting-started chapter per board in RFC-0116 D5's serial-first shape: setup
(`nros setup <board>`), scaffold (the single-package front door, RFC-0116 D6),
build, run in the twin (`nros run`), expected output (`probe-expect`), then the
bench flash section carrying the issue 2176 "unverified" sentence until W5.
The Nucleo chapter gets an ethernet section after the serial one. Each twin
smoke cell (boot, link up, one message heard) joins the matrix through the
generator.

- **Acceptance:** `just probe <track>` passes in a pristine `ros:humble`
  container for each surviving board; the weekly workflow lists the tracks; the
  2169 test has a row per chapter; `check-board-registry` and
  `check-book-board-run-sections` green with no hand-typed board fact outside
  the registry.
- **Decision step:** a track that passes by hand and fails in the container
  for a reason in the emulator (display, licence check, network namespace) is
  an issue against W2's row, not a reason to mark the track manual.

### W5 — Hardware runs

With W0's boards on the bench, flash each chapter's ELF with the chapter's
own flash line (`west flash`, the Pico's UF2 copy, the UNO R4's USB-DFU) and
run the chapter's run section against a host router. Record each in the
registry: `runs` gains `"hardware"`, `last_run` is the date, the log goes to
`docs/roadmap/data/`. The chapter's flash line loses the 2176 sentence in the
same commit the date lands.

- **Acceptance:** three dated hardware runs; the evidence page shows
  "Boots on hardware, manual, <date>" beside the emulator status; no flash
  line in those chapters still says "unverified".
- **Decision step:** a board whose twin passes and whose bench fails is an
  issue against the descriptor, and the chapter's flash section keeps the
  sentence; the twin track stays. A bench pass with a twin fail does not exist
  under RFC-0115 D4 (the board would not be in the book).

## Out of scope

Hardware CI lanes, Arduino GIGA R1 WiFi, Nucleo-H743ZI, ESP32 revival, WiFi on
the Zephyr 3.7 line, Windows-native toolchains (phase-523 documents a container
or WSL road). The education lab series and the adoption probes are phase-523's
successor items and read this phase's tracks.

## Acceptance for the phase

- The book names only boards with a passing twin track.
- `nros setup <board>` installs the twin; no chapter asks the reader to install
  an emulator by hand.
- Every board fact (twin, hardware, cadence, last run, breadth, chapter
  anchors) is typed once, in the registry, and every table and track is
  generated from it.
- Each of the three purposes has a board it can buy, replay in the twin before
  the box arrives, and flash from the same chapter.
