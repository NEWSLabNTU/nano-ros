---
rfc: 0118
title: "Target execution model: run, flash and monitor as separate verbs over one RunPlan, a five-level customisation ladder, and a per-machine bench file"
status: Draft
since: 2026-11
last-reviewed: 2026-11
implements-tracked-by: [phase-528, phase-529]
amends: [0097, 0114, 0116]
supersedes: []
superseded-by: null
---

# RFC-0118 — Target execution model

**Status:** Draft (2026-11-01). Written from the maintainer discussion of
2026-10-31 and 2026-11-01 on how `nros` runs what it built, after a first
proposal of one `run` verb for every target was rejected as unable to cover
real targets. Siblings: [RFC-0117](0117-cli-command-tiers.md) (the command
tree) and [RFC-0119](0119-cli-extension-packages.md) (runner packages, which
carry out everything this RFC resolves). Implemented by
[phase-528](../roadmap/phase-528-run-on-host-and-emulator.md) (host and
emulator) and [phase-529](../roadmap/phase-529-flash-monitor-bench.md) (real
boards).

## Summary

A target is a **launcher or a flasher**, plus a **console**, plus a **link**
for the RMW. `nros` gets four verbs over that model: `run` (a host process or
an emulator), `flash` (program a real board), `monitor` (attach a console
only) and `bench` (record which board on this machine is on which port and
probe), with `debug` reserved. All four consume one **RunPlan**, the same
plan `nros build` prints as its next step and the test harness executes
through `nros run --plan`. The facts behind a plan come from five levels,
highest first: the invocation, the image, the machine, the board, and the
runner's own defaults. Every verb can print the resolved plan and which level
supplied each part.

## Problem

### One verb cannot mean "run" on every target

RFC-0116 D7 asked one verb, `nros run`, to launch an emulator, execute a host
binary, or flash hardware and open the console. A survey of the targets the
tree already has, and of the boards RFC-0115 adds, found five shapes that do
not reduce to one:

| Shape | Examples in or near the tree | What "run" involves |
| --- | --- | --- |
| Host process | Linux/POSIX native, `threadx-linux`, `freertos-posix`, Zephyr `native_sim` | exec a binary; console is stdio |
| Emulator | QEMU across six system binaries (`arm`, `aarch64`, `riscv32`, `riscv64`, the Xilinx fork, `misc`), Arm FVP, Renode and Wokwi (phase-524) | per-machine arguments, NIC choice (SLIRP, bridge, TAP), semihosting, icount |
| Framework-owned flash | Zephyr boards (`west flash` and Zephyr's runner set: J-Link, pyOCD, OpenOCD, nrfjprog, …) | the framework owns the process; nros should hand off |
| Tool-owned flash | ESP32 (`espflash`), RP2040 (`picotool` or UF2 copy), STM32 (`probe-rs`, OpenOCD, DFU) | a tool, a port or a probe, sometimes a bootloader mode |
| Human in the loop | hold BOOTSEL and replug; set BOOT0 and press reset; power-cycle a bench relay | the tool waits on a person, or on a device that appears after they act |

NuttX, FreeRTOS and ThreadX have no runner framework of their own: on real
boards they are flashed by the same tools as bare metal. Zephyr is the only
framework in the tree that owns flashing.

A single verb that covers all five either hides the human step (a lab
session hangs on "waiting for device") or grows a flag per shape. And on
hardware, "run" is really two acts with different lifetimes: flashing ends,
while the console stays open across resets.

### Serial-first boards need two channels

RFC-0116 D5 puts every board's first chapter on USB serial. On a board whose
RMW link is that same serial port, the console and the link compete for one
device. The model has to name the link and the console separately, even
where one cable carries both.

### Every launch is spelled several times

The QEMU command for one board exists as a `runner = "qemu-system-arm …"`
string in `nros-board.toml`, as a line `nros-qemu-ladder` builds for the
build's next-step text (`build_next.rs`), as `nros ws qemu-nic`'s NIC value,
and as `nros_tests::qemu`'s own command builder. RFC-0114 found a board fact
typed in five places; the launch is the same defect for the act of running.

### RFC-0097 D8 rejected these verbs, and its objection still has to be met

RFC-0097 D8 ruled that `nros build` ends at an artifact, that there is no
`nros run` and no `nros flash`, and that what is owed instead is
documentation. Its reason: a run verb "would have to grow a runner per board
and a device story per user". RFC-0116 D7 brought `nros run` back for the
front door without answering that objection. This RFC answers it, and amends
D8, as follows:

- **A runner per board** is real, so runners are not in `nros`. They are
  packages (RFC-0119), one per method rather than per board, and a board
  names its method in one table. The CLI grows a contract, not a runner
  list.
- **A device story per user** is real too, so it is the user's own file:
  `bench.toml` (D6) records this machine's devices and is never shipped.
  nros does not keep a device inventory.
- **The documentation D8 asked for** is still owed, and the plan is what
  provides it. `nros build` prints the resolved command, `--dry-run --why`
  prints where each part came from, and the RunPlan gives the probe chapter
  and the book their command line.

### Prior art

Each of these tools separates the acts this RFC separates:

- **Zephyr west:** `west build -t run` (emulator), `west flash`,
  `west debug`, with a runner per tool chosen by `--runner` and defaulted by
  the board's `board.cmake`.
- **ESP-IDF and PlatformIO:** `flash` and `monitor` are separate targets
  that compose (`idf.py flash monitor`, `pio run -t upload -t monitor`), and
  the port is a flag or auto-detected.
- **Zephyr twister:** a per-machine hardware map records which board is on
  which port and probe; `--flash-command` replaces flashing entirely.
- **cargo and probe-rs:** a configurable runner is any command, given the
  ELF.
- **labgrid and pytest-embedded:** a bench description separate from the
  test, and drivers per flasher and console.

The PlatformIO, arduino-cli and Pico tool details above are recalled, not
re-checked against current documentation.

## Design

### D1 — A target is a launcher or flasher, a console and a link

| Part | Values |
| --- | --- |
| Launcher | host process, emulator (`qemu:<machine>`, `fvp:<model>`, `renode:<platform>`, `wokwi:<board>`) |
| Flasher | a framework runner (`west`) or a tool (`espflash`, `probe-rs`, `openocd`, `picotool`, `uf2`, `dfu-util`, …) |
| Console | `stdio`, `serial`, `rtt`, `semihosting` |
| Link | the RMW transport, as RFC-0109 resolves it: a locator (TCP/UDP) or a serial endpoint |

A target has exactly one launcher or one flasher, one console, and one link.
The console and the link may share a device. Where they do, the runner says
so and nros multiplexes nothing: the image's transport configuration decides
which bytes are which.

### D2 — Four verbs, one reserved

| Verb | Does | Main flags |
| --- | --- | --- |
| `nros run` | launches a host process or an emulator, attaches the console, starts the router if one is needed, streams the console | `--expect <pattern>`, `--timeout <s>`, `--router auto\|none\|<locator>`, `--prefix <cmd>`, `--plan <file>`, `--dry-run`, `--why`, arguments after `--` |
| `nros flash` | programs a real board, then optionally opens the console | `--runner <name>`, `--port <dev>`, `--probe <serial>`, `--no-prompt`, `--monitor`, `--expect`, `--timeout`, `--plan`, `--dry-run`, `--why`, arguments after `--` |
| `nros monitor` | opens the console only | `--port`, `--baud`, `--expect`, `--timeout`, `--send <line>` |
| `nros bench scan\|list` | records which boards are on this machine (D6) | — |
| `nros debug` | reserved for a debugger attach; not designed here | — |

**`nros run` on a hardware image refuses** and names the command that does
what the reader wanted: `nros flash --monitor`. This amends RFC-0116 D7, which
had `run` flash hardware. D7's other properties hold. `run` builds the image if
it is stale, and that build covers the runner package too, which is a
dependency of the image (RFC-0119 D3). `--expect` exits 0 on a match and
non-zero on timeout. The chapter's command line, the printed next step and the
probe's command line are one line.

`--expect` takes a substring by default and a regular expression with
`--expect-regex`; on a match the verb stops the target and exits 0. Exit
codes are the same for all four verbs: 0 success or matched, 1 the target
failed or `--expect` timed out, 2 the runner failed (could not launch, could
not flash), 3 the plan could not be resolved.

### D3 — One RunPlan

A RunPlan is the resolved, serialisable answer to "how is this image run":

```toml
# <build>/nros/run/<image>/plan.toml (JSON on the runner contract)
image    = "talker"
artifacts = { elf = "…/talker.elf", bin = "…/talker.bin" }
verb     = "run"
runner   = { package = "nros_runner_qemu", name = "qemu" }
target   = { emulator = "qemu:mps2-an385" }
console  = { kind = "semihosting" }
link     = { kind = "tcp", locator = "tcp/10.0.2.2:7447" }
args     = { nic = "user,model=lan9118" }
prepare  = []
provenance = { "target.emulator" = "board", "args.nic" = "image" }
```

It has three consumers, and none of them derives a launch on its own:

1. `nros build` prints the plan's command as its next step. This replaces
   `build_next.rs`'s `NextSteps` derivation; the issue 2169 test keeps
   holding the printed line, the chapter's line and the probe's line equal.
2. `run`, `flash` and `monitor` resolve a plan and hand it to a runner
   (RFC-0119 D6).
3. The test harness writes a plan and calls `nros run --plan <file>` (or
   `nros flash --plan`). The harness and a user then go through the same path,
   including router start, console capture and `--expect` grading. That
   replaces `nros_tests::qemu`'s command builder, the ESP32 launcher and the
   ad-hoc native spawns.

`--plan <file>` executes a plan as written and resolves nothing. `--dry-run`
prints the plan and runs nothing. `--why` adds the provenance of each field.

### D4 — Where the facts live

| Fact | Owner |
| --- | --- |
| how the board is launched, flashed and observed: emulator machine, flash method, chip, USB VID:PID, baud, prepare steps | the board package's `nros-board.toml`: `[board.run]`, `[board.flash]`, `[board.console]` |
| how to drive a method | the runner package (RFC-0119) |
| which port and probe this machine uses | `$NROS_HOME/bench.toml` (D6) |
| this image's override | `system.toml`: `[image.<id>.run]`, `[image.<id>.flash]`, `[image.<id>.console]` (the `run` table exists today for NIC choice) |
| the link | the image's transport and RFC-0109's network layers; no new table |
| whether the board was verified | the registry (RFC-0114), never the descriptor |
| Zephyr's flash and emulator facts | Zephyr's own `board.cmake` and `runners.yaml`, which the `west` runner reads |

```toml
# nros-board.toml
[board.run]                       # present when the board has an emulator or host form
emulator = "qemu:mps2-an385"      # RFC-0114 vocabulary; or host = true
[board.run.qemu]                  # runner-specific facts, read only by that runner
cpu = "cortex-m3"
nic_model = "lan9118"
semihosting = true

[board.flash]                     # present when the board is real silicon
method  = "probe-rs"
chip    = "STM32F767ZITx"
prepare = []

[board.console]
kind = "serial"
usb  = "0483:374b"
baud = 115200
```

**The registry's `emulator` field becomes derived.** RFC-0114 D10 types
`emulator = "qemu:<machine>"` in `board-support.toml`. A board outside the
tree has no registry row and still needs to run, so the fact must live in the
descriptor. RFC-0114 D12's own rule — no board fact typed twice — then makes
the registry read it from the descriptor. This amends RFC-0114 D10. The
registry keeps `runs`, `hardware`, `cadence` and `last_run`, which are
evidence. phase-528 W2 carries the change together with phase-522's
generator.

The `runner = "…"` strings in today's descriptors retire into
`[board.run]` and its runner table (phase-528 W3).

### D5 — The customisation ladder

Highest precedence first:

```
1. invocation   --runner, --port, --probe, arguments after `--`
2. image        [image.<id>.run|flash|console] in system.toml
3. machine      $NROS_HOME/bench.toml
4. board        [board.run|flash|console] in nros-board.toml
5. method       the runner package's own defaults
```

The ladder has the shape of RFC-0049's knob ladder, and `--why` reports it the
way `nros config explain` and `nros locate --why` already do.

At levels 2 and 4 a table may replace the method with a **command template**,
for a tool no runner covers:

```toml
[image.lab.flash]
command = "pyocd flash -t stm32f767zi {hex}"
prepare = [{ say = "Set BOOT0 to 1, then press RESET", wait_for = "usb:0483:df11" }]
```

A template is executed by the `command` runner (RFC-0119), so it gets the same
console, `--expect` and cleanup handling as any other runner. Placeholders
come only from the plan: `{elf}`, `{bin}`, `{hex}`, `{uf2}`, `{build_dir}`,
`{port}`, `{baud}`, `{probe}`, `{nic}`. `{tool:<name>}` resolves a tool
through the SDK store, never through `PATH`. An unknown placeholder is
refused, not passed through.

### D6 — The bench file and port resolution

`nros bench scan` enumerates attached USB serial devices and probes, matches
them against the `usb` VID:PID of every board the workspace names, and
writes `$NROS_HOME/bench.toml`:

```toml
[[board]]
name   = "nucleo_f767zi"
serial = "066DFF494851877267084535"   # USB serial number, stable across replug
port   = "/dev/serial/by-id/usb-STMicroelectronics_…"
probe  = "066DFF494851877267084535"
power_cycle = "./relay.sh 3"           # optional
```

The file belongs to one machine and is never committed: a teammate's
`/dev/ttyACM0` is not yours. `nros bench list` prints it, with whether each
device is present now.

Port resolution, first answer wins: `--port`, `NROS_PORT`, the bench file,
exactly one attached device matching the board's VID:PID. If none of these
gives an answer, the verb refuses and lists what it found. It never guesses
between two devices.

### D7 — Human steps

`prepare` is an ordered list. Each step is one of:

- `say = "<text>"`: print the instruction;
- `wait_for = "<event>"`: block until the event happens, where an event is
  `usb:<vid>:<pid>`, `port:<path>`, `volume:<label>` (a UF2 mass-storage
  mount), or `line:<text>` (on the console);
- `run = "<command>"`: run a host command (a relay, a power switch).

`say` and `wait_for` usually come as a pair. With no terminal, or with
`--no-prompt`, a `say` step fails the verb with exit 2 and prints the
instruction. A lab script or CI job then gets a clear failure instead of a
hang. Steps are a list, not a state machine. A bench that needs board states
(labgrid's strategy model) belongs in a runner package.

### D8 — Router, console capture and lifetime

- `--router auto` (default) starts the zenoh router when the plan's link is a
  locator on this host or a serial endpoint, through the existing
  `nros_router_exec` resolution (RFC-0075); `none` starts nothing; a locator
  points at an existing router. Other RMWs state their own need through the
  link (an XRCE image needs an agent, which phase-528 W4 handles through the
  same switch).
- Every verb tees the console to `<build>/nros/run/<image>/console.log`.
- The target runs in its own process group. Ctrl-C, `--timeout`, a matched
  `--expect` and verb exit all stop the whole group, router included.

## Alternatives considered

- **One `run` verb for every target (RFC-0116 D7 as written).** Rejected for
  the reasons in the Problem: human steps, the flash/console lifetime split,
  and the framework hand-off.
- **Platform-specific verbs (`nros zephyr flash`, `nros esp32 flash`).** The
  board already says which platform it is on. A verb per platform puts the
  platform choice back on the user and doubles the help text.
- **Board-only facts, no image or machine levels.** That is where the tree is
  today, with `runner = "…"` strings in descriptors. It forces a user with
  another probe to fork the board package, and a lab with a relay to edit a
  shared file.
- **Run facts in the registry only.** An out-of-tree board has no registry
  row (D4).

## Open questions

1. `nros debug`: attach a GDB server through the runner (`west debug`,
   `probe-rs gdb`, QEMU `-s -S`), or only print the command? Deferred until a
   runner needs it.
2. A bringup with several images (a talker and a listener in two QEMUs):
   does `nros run` start every image of the bringup, or one per invocation
   with `--image`? Starting one is the smaller promise; the harness needs
   several.
3. Should `wait_for` gain `tcp:<host>:<port>` for an emulator that opens a
   socket after boot (Renode's monitor), or is that the runner's job?

## Changelog

- 2026-11 — created, from the maintainer discussion of 2026-10-31 and
  2026-11-01. Amends RFC-0097 D8 (the verbs exist, as a contract over
  runner packages), RFC-0116 D7 (`run` does not flash) and RFC-0114 D10
  (the registry's `emulator` is read from the descriptor).
