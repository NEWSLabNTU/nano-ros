# Phase 578 — what peer toolchains give a user that nano-ros does not yet

**Status (2026-11-18). OPEN: W1–W11 are open and unassigned; nothing is built
yet.** Filed from a survey the user asked for on 2026-11-18: how Rust/cargo,
Arduino, PlatformIO, ESP-IDF, Zephyr, Embassy/probe-rs and vendor tools (TI
SysConfig, STM32CubeMX, nRF Connect for VS Code, Segger SystemView, Percepio
Tracealyzer) deliver their user experience, and where nano-ros falls short.
The managers assign each item. Items that need a design say so; the others
are one PR each.

## Why

Each item below is a habit a user brings from one of those tools, and each was
checked against nano-ros main on 2026-11-18:

- **After a build, Arduino, Zephyr and nRF Connect print flash and RAM used
  against the board's capacity.** `nros build` prints neither.
- **ESP-IDF's monitor turns a crash's addresses into source lines, and
  probe-rs prints a backtrace on panic.** Nothing in the CLI or the runner
  packages symbolizes an address.
- **cargo's `Cargo.toml` has a JSON Schema in SchemaStore, so any editor with
  a TOML language server completes and checks it.** `system.toml` and
  `nros-board.toml` have none.
- **rustc points at the line that is wrong, and `cargo fix` / clippy apply a
  suggested fix.** nano-ros errors carry a diagnostic code and `nros explain`
  (RFC-0129), but they name a file, not a line, and nothing applies a fix.
- **Arduino's File → Examples and Zephyr's samples list what runs on the
  chosen board.** nano-ros has no way to list examples per board (phase-577 W7
  adds one example per board key, not a way to find it).
- **PlatformIO's `pio test` and Zephyr's twister run a project's tests on the
  emulator or the board.** nano-ros's probes and `nros run` do this for the
  book, but a user has no verb for their own tests.
- **Zephyr's board catalog is a page a user filters by architecture, vendor
  and feature.** nano-ros generates a support-levels table, unfiltered.
- **An Arduino user's first blink takes about two minutes.** A cold first
  nano-ros build compiles zenoh-pico, lwIP and the RTOS from source, several
  minutes per image.
- **nRF Connect and PlatformIO put board choice, build, flash and monitor in
  the editor.** nano-ros writes `compile_commands.json` for FreeRTOS, NuttX
  and ThreadX C and C++ images (release 0.6.0) and Zephyr's build always has,
  and has no editor integration.
- **Embassy's defmt sends log arguments as indices and formats them on the
  host,** so small boards log cheaply. nano-ros formats on the target.
- **SystemView and Tracealyzer show tasks and deadlines on a timeline.**
  nano-ros has none, and phase-575 W5's deadline monitors will produce the
  events worth showing.

What nano-ros already does as well as any of them, and keeps: book pages
replayed by probes in clean containers; retired spellings refused with their
replacement named; `nros flash` picking the one attached device that matches
the board (`nros bench`); one-line board switching with every refusal reason
at once (RFC-0135, draft).

## Work items

### W1 — a size report after every build

- **Status:** OPEN
- **Owner:** unassigned

`nros build` ends with the image's flash and RAM use, from the ELF's section
sizes, and the largest pools from the sizing descriptor (RFC-0100). Against
the board's capacity once the board definition states one (RFC-0135 D8's
memory budget, phase-577 W6); in absolute numbers until then.

Acceptance: a FreeRTOS QEMU build prints flash and RAM used, and with a stated
budget the percentages; a test holds the numbers equal to the ELF's.

### W2 — crash addresses as source lines in `nros monitor`

- **Status:** OPEN
- **Owner:** unassigned
- **Needs:** a short design note: which fault formats (Cortex-M HardFault
  frame, Cortex-R abort, Rust panic location, a POSIX backtrace) and where the
  ELF comes from when the monitor did not build the image.

`nros monitor` recognises a fault dump or panic line in the console, looks
the addresses up in the image's ELF, and prints function, file and line under
it.

Acceptance: an injected fault on the FreeRTOS QEMU board prints the faulting
function and line.

### W3 — JSON Schemas for `system.toml` and `nros-board.toml`

- **Status:** OPEN
- **Owner:** unassigned

Generate a JSON Schema for each file from the sources the CLI already reads:
the knob catalog (phase-569 W2) for `[image.<id>.knobs]`, the board
definition's schema for `nros-board.toml`. Ship them with the SDK, and point
editors at them (a `#:schema` line in scaffolded files; a SchemaStore entry
once the URLs are stable).

Acceptance: a scaffolded project opened in an editor with a TOML language
server completes knob names and flags an unknown key; a gate holds the
schemas equal to their sources.

### W4 — errors that point at the line, and `nros fix`

- **Status:** OPEN
- **Owner:** unassigned
- **Needs:** a design (an RFC-0129 amendment): the span format, and which
  refusals carry a machine-applicable fix.

Configuration errors name the file, line and key, with the line shown, in the
style rustc uses. A refusal whose remedy is mechanical (a retired spelling, a
knob in the wrong table, RFC-0135 D8's `fix:` lines) is applied by
`nros fix`, which prints the diff first.

Acceptance: a retired knob spelling in `system.toml` is reported at its line,
and `nros fix` rewrites it.

### W5 — examples per board, from the CLI

- **Status:** OPEN
- **Owner:** unassigned
- **After:** phase-577 W7

`nros new --example <name> --board <key>` copies a board package's example
into a new project, and `nros board show <key>` lists the examples. The
examples come from the board package (RFC-0135 D7).

Acceptance: every in-tree board key lists at least one example, and each
copies and builds.

### W6 — `nros test`

- **Status:** OPEN
- **Owner:** unassigned
- **Needs:** a design note: how a test states what it expects (the probes'
  `probe-expect` output lines, or an exit code from the image), and where
  hardware runs fit (`nros bench`).

Run a project's test images on their target (host, emulator, or an attached
board), grading each by its console output, with one summary.

Acceptance: a scaffolded project's test image passes on the FreeRTOS QEMU
board and on the host, and a failing one is reported with its output.

### W7 — a board catalog page a user can filter

- **Status:** OPEN
- **Owner:** unassigned

Generate a page from the board registry, with filters for architecture, RTOS,
RMW, emulator and support status, linking each board to its book chapter and
examples. The generated support-levels table stays the source.

Acceptance: the page lists exactly the registry's boards (a gate compares
them), and works without a server.

### W8 — a shorter first build

- **Status:** OPEN
- **Owner:** unassigned
- **Needs:** an RFC: what is prebuilt (zenoh-pico, lwIP, the RTOS kernel, the
  nano-ros C and Rust libraries), per which key (board, RMW, knobs that change
  the object code), and how a prebuilt piece is checked against the source it
  claims to be.

Ship prebuilt, per-board libraries in the store, the way the PX4 POSIX
archives already ship (phase-541), so a first build compiles only the user's
code.

Acceptance: from a clean store, the tutorial's first FreeRTOS QEMU build
completes in a stated time, measured by the probe.

### W9 — an editor integration

- **Status:** OPEN
- **Owner:** unassigned

First: `compile_commands.json` for every image kind (check which of the
POSIX and bare-metal C and C++ images lack one), and a `rust-project.json`
or rust-analyzer settings for Rust images, so an editor resolves symbols
without configuration. Later, and only with a user decision: a thin editor
extension for board choice, build, flash and monitor.

Acceptance: a scaffolded C, C++ and Rust project on each RTOS opens in an
editor with symbols resolved.

### W10 — compact logging for small boards

- **Status:** OPEN
- **Owner:** unassigned
- **Needs:** a design note against `nros-log` (RFC-0102): which log calls are
  interned, where the string table lives, and how `nros monitor` finds it.

Log arguments travel as indices into a string table in the ELF, formatted by
`nros monitor` on the host, on the pattern of defmt.

Acceptance: on the FreeRTOS QEMU board, an image logging through the compact
path is smaller and prints the same lines in `nros monitor`.

### W11 — a timeline of tasks and deadlines

- **Status:** OPEN
- **Owner:** unassigned
- **After:** phase-575 W5
- **Needs:** an RFC: the event set and the output format (SystemView,
  Perfetto, or CTF).

Record executor and scheduling events (task start and end, budget overrun,
deadline miss) and export them in a format an existing viewer opens.

Acceptance: a QEMU run's trace opens in the chosen viewer and shows an
injected deadline miss.

## Order

W1, W3, W5 and W7 are small and independent, and the most visible: start
there. W2, W4 and W6 each need their design note first. W8 and W11 need an
RFC. W9's first half is small; its second half waits for a user decision.

## Related

- RFC-0129 (diagnostic codes), RFC-0133 and phase-569 (discoverability),
  RFC-0135 and phase-577 (board packages), RFC-0100 (sizing), RFC-0102
  (logging), phase-575 W5 (deadline monitors), phase-541 (prebuilt PX4
  archives).

## Issues homed here

| issue | why it belongs here |
| --- | --- |
