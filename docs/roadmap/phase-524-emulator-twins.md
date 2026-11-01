# Phase 524 — Emulator twins for the boards adopters hold

**Status (2026-11-01). IN PROGRESS. W1 done: one survivor, the Nucleo-F767ZI (serial and ethernet in Renode 1.17.0); the UNO R4 Minima failed (does not fit the RA4M1) and the Pico W was blocked (no `WOKWI_CLI_TOKEN`, no RP2040 in Renode); the Pico W rerun with a token PASSED over serial, so the Pico W is back as the field solutions board (W2 to W4 follow-ups listed under W1; issue 2206 first). RFC-0115 D4 rewritten to match. W2 and W3 ran for the Nucleo only. W2 done: `[tool.renode]` (Linux x86_64 and arm64; macOS has no portable archive) and `hal_stm32` on the 3.7 line; the board row that names them is W3. W3 done: the `nucleo-f767zi-zephyr` package and registry row (manual, 2026-11-01; weekly after W4's probe track), built through the leaf road and booted in Renode. W2 done for the Pico W: `[tool.wokwi-cli]` (all four host keys) and `hal_rpi_pico` on the 3.7 line; `thumbv6m-none-eabi` landed with issue 2206. The Pico W's W3 item, W0 and W4 open.** Implements
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
| 3. Pico W, rerun | PASS, serial | `data: 'Hello World: 1'` from `ros2 topic echo`, the Pico W ELF in Wokwi, zenoh over UART0 to `wokwi-cli --interactive`, a host pty and `serial/` on the router | [wokwi-pico-rerun](data/phase-524-w1-wokwi-pico-rerun.log) |

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

**Rerun of spike 3 (2026-11-01, with a Wokwi token). PASS, serial road.**
The maintainer created a Wokwi token (personal account, free plan), and spike
3 ran again with the same `wokwi-cli` 0.29.1 binary, the C talker for
`rpi_pico/rp2040/w`, Zephyr 3.7.0 and `hal_rpi_pico` as an extra module.
Decisive line: `data: 'Hello World: 1'` from `ros2 topic echo /chatter`, the
first sample 6 s after the simulation started, twice on the same ELF. Log:
[wokwi-pico-rerun](data/phase-524-w1-wokwi-pico-rerun.log). The first run's
log stays as it was.

- **The link.** Wokwi's serial monitor is UART0 (GP0/GP1). `wokwi-cli
  --interactive` puts the CLI's stdin on the UART's RX and the serial monitor
  on its stdout, both byte-transparent (zenoh-pico's COBS frames keep their
  0x00 delimiters). `socat` turns that pair into a host pty, and
  `ros2 run rmw_zenoh_cpp rmw_zenohd` opens the pty as a `serial/` locator.
  This is the Nucleo chapter's shape, with `wokwi-cli` in place of the
  ST-LINK virtual COM port. The console moves to UART1 (GP4/GP5). On the bench
  the zenoh link is the GP0/GP1 header pins, so it needs a USB-to-UART
  adapter: the Pico W's USB port is not that UART.
- **WiFi road: not tried.** Serial passed. A host-local router over the Pico W
  WiFi needs Wokwi's private IoT gateway, which the free (Community) plan does
  not include; the private gateway starts at Hobby.
- **The image needs `CONFIG_TICKLESS_KERNEL=n`.** With the board default
  (tickless), Wokwi shows no byte, and Zephyr's own `hello_world` shows none
  either. Under `wokwi-cli -g`, gdb showed UART0 and its pins set up
  correctly, every stop in the SysTick ISR, and `z_sys_post_kernel = false`:
  the kernel never leaves PRE_KERNEL. Wokwi's SysTick does not carry Zephyr
  3.7's tickless `cortex_m_systick` driver. With `CONFIG_TICKLESS_KERNEL=n`,
  `hello_world` prints and the talker passes. The bench can run that image
  too, so "the same ELF on the bench and the twin" holds once the descriptor
  states it. It is not the board default.
- **Three tree gaps, patched only locally (issue 2206).** There is no
  Cortex-M0+ arm in `nros_detect_rust_target()`. Two core-atomics
  read-modify-write sites (`nros-platform-cffi` `log.rs`, `nros-cpp`
  `subscription.rs`) do not build without CAS. Nothing provides the ARMv6-M
  `__atomic_*_4` libcalls that zenoh-pico's C needs. Patches and shim were
  reverted; none is on main.
- **RAM (264 KiB).** The default pools overflow by 211,736 bytes. The image
  needed `ZENOH_MAX_LARGE_SUBSCRIBERS = 0`, `ZENOH_MAX_SUBSCRIBERS = 1`,
  `ZENOH_SERVICE_BUFFER_SIZE = 1024`, `ZEPHYR_HEAP_SIZE = 32768` and
  `ZENOH_MAX_QUERYABLES = 1` to fit, at 94.3 % RAM. The largest remaining
  block is the 83,880-byte executor backing.
- **Plan limits.** The free plan gives 50 simulation minutes per rolling 30
  days and 5 minutes wall clock per session. Hobby / Hobby+ / Pro give
  200 / 500 / 2,000 minutes, with sessions of 5 / 10 / 50 minutes. A twin run
  is about 1 to 1.5 minutes, so a weekly cadence uses about 6 of the 50:
  viable. Two caveats. The quota is per user, so CI shares it with anyone
  developing on the same account (this spike used about 8 minutes). And the
  token is a repository secret that a fork pull request cannot read, so the
  track runs on schedule and dispatch only.

Decision applied (the decision step, literally): Wokwi passes, so the Pico W
rejoins RFC-0115 D4 as the field solutions board. D4's table gains the Wokwi
row, with its conditions. Follow-ups, each in its work item's shape:

- **Before W3: issue 2206.** The Pico W ELF has to build from main with no
  local patch.
- **W2: `[tool.wokwi-cli]`.** A dist row in `[tool.renode]`'s shape: version
  `0.29.1`, upstream's per-host static binaries (we apply nothing, so no
  repack), a `front` link and a `--version` smoke. The floor is measured off
  the artifact, and the hosts with no asset are stated. Also
  `[zephyr_module.hal_rpi_pico]` moves to `lines = ["3.7"]`, and
  `thumbv6m-none-eabi` joins `examples/zephyr/rust-toolchain.toml`. `socat`
  comes from the probe image. The track needs both.
- **W3: a `wokwi:pi-pico-w` row and a descriptor.**
  `packages/boards/nros-board-rpi-pico-w-zephyr/`, with `west_board =
  "rpi_pico/rp2040/w"` and `rust_target = "thumbv6m-none-eabi"`. It carries
  an overlay that moves the console to UART1 and gives UART0 to zenoh, the
  serial Kconfig fragment plus `CONFIG_TICKLESS_KERNEL=n`, and the five RAM
  knobs in `[knobs]`, measured against the boot report before they settle.
- **W4: the `wokwi-pico` track.** `wokwi-cli -q --interactive` bridged by
  `socat` to the router's `serial/` locator, as in the log. It needs a
  `WOKWI_CLI_TOKEN` repository secret, and runs on schedule and dispatch
  only, weekly.

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

**Result, Pico W (2026-11-01). Done; the board link waits on W3.** The W1
rerun brought the Pico W back, so W2 now adds the twin's client and the HAL.

- **`[tool.wokwi-cli]`**, version `0.29.1`, upstream's own binaries (we apply
  nothing, so no repack). Each asset is a bare executable, not an archive, so
  each dist carries an `install` (the `[tool.ninja]` shape) that puts it at
  `bin/wokwi-cli`; `front = ["bin/wokwi-cli"]` links it into `$NROS_HOME/bin`,
  and `smoke` runs `bin/wokwi-cli --version` and expects `0.29.1`. sha256s are
  the release's own `digest` fields; floors are measured off the binaries.

  | host | asset | sha256 | floor (measured off the artifact) |
  |---|---|---|---|
  | linux-x86_64 | `wokwi-cli-linuxstatic-x64` | `3c9d6da0...4258` | none: static-pie |
  | linux-arm64 | `wokwi-cli-linuxstatic-arm64` | `ef442aad...eeee` | none: static-pie |
  | macos-arm64 | `wokwi-cli-macos-arm64` | `94da96ee...3eb3d` | macOS 11.0 |
  | macos-x86_64 | `wokwi-cli-macos-x64` | `27642917...96cb3` | macOS 10.15 |

  Unlike Renode it is a cloud client: a run needs network access and a
  `WOKWI_CLI_TOKEN`, under the plan limits W1 recorded (free: 50 simulation
  minutes per rolling 30 days, 5 minutes per session, per user). The row
  comment says so. `--version` needs neither: it answers offline and writes
  nothing to `HOME`.
- **`[zephyr_module.hal_rpi_pico]`** is new (the module had no row at all):
  `lines = ["3.7"]`, `needed_by = ["rpi_pico"]`, `approx_mb = 11` (the spike's
  shallow checkout), and `west.yml`'s allowlist names it. Its revision is
  zephyr v3.7.0's own pin, `fba7162cc7bee06d0149622bbcaac4e41062d368`.
- **`thumbv6m-none-eabi`** in `examples/zephyr/rust-toolchain.toml` (and the
  root toolchain, `config/rust-targets.txt` and the index) landed with issue
  2206's fix, because `check-rust-targets-covered` requires it beside the new
  `nros_detect_rust_target()` arm.
- **Measured.** `nros setup --tool wokwi-cli --dry-run` plans `prebuilt 0.29.1
  (dist linux-x86_64)`; `nros setup zephyr --dry-run` prices `hal_rpi_pico
  11 MB [3.7] rpi_pico`. A real `nros setup --tool wokwi-cli` into the box
  store, then `$NROS_HOME/bin/wokwi-cli --version` under `env -i` with an empty
  `HOME`, prints `0.29.1 (0aa185144b92)` and leaves that `HOME` empty.
  `check-dist-floors`, `check-smoke-or-reason`, `check-dist-or-reason` and
  `check-zephyr-module-allowlist` green. One gap in the tooling, not the row:
  `measure-dist-floor.py` reads only tar and zip, so a bare-binary asset is
  measured with `--dir` over the downloaded file.
- **Left.** `socat` (the pty bridge) is already `[prereq.socat]` in the index;
  putting it in the probe container is W4's, with the track that uses it. The
  Pico W `[board.*]` row that names `wokwi-cli` in `packages` is W3's.

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

**Result (2026-11-01). Done for the Nucleo-F767ZI.** One row, one package.

- **Registry row** (`packages/boards/board-support.toml`):

  ```toml
  [[board]]
  crate = "nros-board-nucleo-f767zi-zephyr"
  board_keys = ["nucleo-f767zi-zephyr"]
  real_hardware = true
  book_flash = "getting-started/integration-zephyr.md#flash-a-real-board"
  runs = "emulator"
  emulator = "renode:nucleo_f767zi"
  hardware = "nucleo-f767zi"
  cadence = "manual"
  last_run = "2026-11-01"
  breadth = "inherits ZephyrQemuCortexM"
  maintainers = ["jerry73204"]
  ```

  Derived status: "Boots, manual (2026-11-01)". `cadence` is `manual` because
  the evidence is the W1 spike run plus the W3 rerun below, both by hand. It
  moves to `weekly` with `probe = ["renode-nucleo"]` when W4's generated
  track replays it. `breadth` inherits the Zephyr Cortex-M witness, not
  `witness`: the twin adds one smoke run on the family's code path and asserts
  no matrix of its own (RFC-0115 D4, RFC-0114 D11). The row has no
  `matrix_platform` because no `PlatformId` runs it, and a manual row may say
  that. The maintainer is named, so the row is not in the grandfather list.
  The plan above said `cadence = "weekly"`. That waits for W4's track, because
  the gate requires a `probe` track for `weekly`.
- **`book_run` is W4's.** The grammar requires `book_run` on a row that runs,
  unless `real_hardware = true`. The Nucleo is flashed silicon, so the row
  states that and names the existing "Flash a real board" section of the
  Zephyr chapter (it already used `nucleo_f767zi` as its example). That is the
  least-scope legal option. The twin's "Run" section comes with W4's chapter,
  and that commit adds `book_run`. A run block written now would be an
  unprobed command. One sentence of that section changed: it said nano-ros
  ships no Zephyr hardware board.
- **Package** `packages/boards/nros-board-nucleo-f767zi-zephyr/`:
  `nros-board.toml` (`west_board = "nucleo_f767zi"`,
  `rust_target = "thumbv7em-none-eabihf"`, `stack = "os"`,
  `links = ["serial", "tcp", "udp"]`, `zephyr_line = "3.7"`), `package.xml`,
  `prj.conf` (the spike's serial set plus the POSIX half of `prj-zenoh.conf`,
  so a serial image names no `os_config`), and
  `boards/nucleo_f767zi.overlay` (console on USART6, zenoh on USART3). The
  board key also joins `BOARD_KEYS`, `BOARD_PATHS`, the zephyr framework
  table and issue 1435's refused set. `[board.nucleo-f767zi-zephyr]` in
  `nros-sdk-index.toml` names `zephyr-sdk` and `renode`.
  `examples/zephyr/rust-toolchain.toml` gains `thumbv7em-none-eabi(hf)`.
  `nros_detect_rust_target()` already had the Cortex-M7 arm.
- **Knobs: not in `[knobs]`, issue 2194.** The descriptor cannot state the
  four RAM knobs. Its `[knobs]` schema has no field for them, and the Zephyr
  leaf road has no board knob rung. A prompt-less knob in `prj.conf` is a
  configure error. So the descriptor header records the numbers and the
  ethernet heap's shortfall: 32 KiB against a 39,728-byte peak plus floor.
  An image states `[image.<id>.knobs.zenoh] max_large_subscribers = 0`.
- **Two leaf-road defects, fixed here because the build needed them.** (1) A
  leaf image got its board package's Kconfig (issue 1782) but not its
  devicetree overlay. A nucleo image built with the console and the zenoh link
  on one UART. `nros ws leaf-system` now prints `NROS_LEAF_DTC_OVERLAY` and
  the module hook adds it to `EXTRA_DTC_OVERLAY_FILE`. (2) The board facts
  resolver reads `NROS_LEAF_IMAGE_ID`, which nothing on the Zephyr leaf road
  set. So a leaf whose images name two boards failed with "deploys ...
  resolve DIFFERENTLY", even with `-DNROS_IMAGE` given. The image hook now
  sets it.
- **Build proof.** A copy of `examples/zephyr/c/talker`
  (`tmp/w3-nucleo/talker`, untracked) with one image added:

  ```toml
  [image.nucleo_serial]
  board = "nucleo-f767zi-zephyr"
  rmw = "zenoh"
  locator = "serial/serial@40004800#baudrate=115200"

  [image.nucleo_serial.knobs.zenoh]
  max_large_subscribers = 0
  ```

  `west build -b nucleo_f767zi <leaf> -- -DNROS_IMAGE=nucleo_serial
  -DZEPHYR_EXTRA_MODULES=<repo>;<hal_stm32>` passed no conf file and no
  overlay. The `hal_stm32` path is there only because this box's workspace
  manifest predates W2. The merge order was the leaf's `prj.conf`, the
  board's `prj.conf`, then the rendered image. The overlay came from the board
  package, and the console is `&usart6`. RAM was 351,936 B (89.50 %), the
  spike's number to the byte. FLASH was 522,088 B. That ELF (sha256
  `a66df2b7...0228`) booted in Renode 1.17.0 through the spike's
  `run-serial.sh`. `ros2 topic echo` printed `data: 'Hello World: 1'`, so the
  run PASSED.
- **`nros setup nucleo-f767zi-zephyr --dry-run`** prices `hal_stm32 639 MB
  [3.7] nucleo_f767zi`, `rust target thumbv7em-none-eabihf`, `zephyr-sdk` and
  `renode present 1.17.0`.
- **Left for W4.** The `renode-nucleo` track, `probe` plus `cadence = "weekly"`
  on the row, `book_run`, the chapter, and an ethernet image. The ethernet
  image needs an `os_config` that turns the IP stack back on, since this
  board's `prj.conf` is serial-shaped. It also needs its heap measured first,
  see issue 2194. The Nucleo's `nros board list` status reads "Boots, manual"
  until then, not "Builds".

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
- **Run commands (2026-11-01).** The chapter's run line is `nros run` in the
  twin and `nros flash --monitor` on the bench (RFC-0118 D2). The twin is
  driven by an `nros_runner_renode` package written against the runner
  contract (RFC-0119, phase-528), not by a special case in the CLI, and the
  bench run uses phase-529's verbs.

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
