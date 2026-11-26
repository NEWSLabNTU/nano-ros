# Phase 578 — what peer toolchains give a user that nano-ros does not yet

**Status (2026-11-18). OPEN: the user approved the phase on 2026-11-18; W1–W11
are assigned and nothing is built yet.** Filed from a survey the user asked for on 2026-11-18: how Rust/cargo,
Arduino, PlatformIO, ESP-IDF, Zephyr, Embassy/probe-rs and vendor tools (TI
SysConfig, STM32CubeMX, nRF Connect for VS Code, Segger SystemView, Percepio
Tracealyzer) deliver their user experience, and where nano-ros falls short.
Owners as the manager assigned them on 2026-11-18; each starts after its
owner's current batch. Items that need a design say so; the others are one
PR each.

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
- **Owner:** nros 133, after its current batch

`nros build` ends with the image's flash and RAM use, from the ELF's section
sizes, and the largest pools from the sizing descriptor (RFC-0100). Against
the board's capacity once the board definition states one (RFC-0135 D8's
memory budget, phase-577 W6); in absolute numbers until then.

Acceptance: a FreeRTOS QEMU build prints flash and RAM used, and with a stated
budget the percentages; a test holds the numbers equal to the ELF's.

### W2 — crash addresses as source lines in `nros monitor`

- **Status:** OPEN
- **Owner:** nros 241, after its current batch
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
- **Owner:** nros 139 AUO book, after phase-569 W2, which it builds on

Generate a JSON Schema for each file from the sources the CLI already reads:
the knob catalog (phase-569 W2) for `[image.<id>.knobs]`, the board
definition's schema for `nros-board.toml`. Ship them with the SDK, and point
editors at them (a `#:schema` line in scaffolded files; a SchemaStore entry
once the URLs are stable).

Acceptance: a scaffolded project opened in an editor with a TOML language
server completes knob names and flags an unknown key; a gate holds the
schemas equal to their sources.

### W4 — errors that point at the line, and `nros fix`

- **Status:** OPEN — in progress; design accepted 2026-11-19 (user ruling): [RFC-0129](../design/0129-diagnostic-codes.md) "Amendment (phase-578 W4)" (D7 locations, D8 `nros fix`, D9 gates), with its four drafted answers
- **Owner:** nros 118, after its current batch
- **Needs:** a design (an RFC-0129 amendment): the span format, and which
  refusals carry a machine-applicable fix.

Configuration errors name the file, line and key, with the line shown, in the
style rustc uses. A refusal whose remedy is mechanical (a retired spelling, a
knob in the wrong table, RFC-0135 D8's `fix:` lines) is applied by
`nros fix`, which prints the diff first.

Acceptance: a retired knob spelling in `system.toml` is reported at its line,
and `nros fix` rewrites it.

**Slice 1 (2026-11-19): the acceptance shape, end to end.**
- `Location` and `Refusal::at` in `nros-diagnostic-codes` render rustc's span
  block (`-->`, gutter, carets) under the code line; `Refusal::fixable` adds
  the `fix: nros fix` line.
- `nros-cli-core::source_file::SourceFile::locate(key_path)` re-reads a
  file's own text with `toml_edit` and returns the key's span. No serde
  structure changed.
- `nros-cli-core::fix`: a `Finding` (refusal plus optional `Fix`), the
  `RenameKey` edit, and one collector both consumers share. It reads every
  file a workspace's `system.toml`s reach, includes too. NROS-0011
  `RetiredKnobSpelling` covers `[image.<id>] env` and `[image_defaults] env`;
  a knob retired outright gets no fix.
- `nros build` and `nros configure` run the collector as a preflight, so the
  refusal arrives at its line before any reader meets the row.
- `nros fix [PATH] [--apply]` prints a unified diff and writes only with
  `--apply`. It refuses two fixes on one span, or a rename onto a key the
  table already states, before writing anything. A file outside the
  workspace gets no fix.

Left for W4:
- `MoveKey` (an `[image] env` row naming a registry knob, which belongs in
  `[image.<id>.knobs.<group>]`; today `app_rung` refuses it with no line),
  `ReplaceValue` (`native`/`posix` as the host board) and `SetKey`, each with
  its fixture;
- locations for `nros-board.toml`;
- the secondary `note_at` location, and refusal groups;
- D9's `check-diagnostics` rule (an entry that declares a fix kind names
  `nros fix` in its explanation);
- the [RFC-0140](../design/0140-configuration-registry.md) D4 contract
  (agreed 2026-11-19, built by phase-581), in this order:
  - `RenameKey` gains `scale`, and `MoveKey` is
    `{ to_table, to_key, scale }`. The new value is the old value times the
    factor, and an edit whose result falls outside the new row's type or
    range is refused, naming the limit.
  - The edits cover the `toml:` and `env:` roads. A `kconfig:` or `cmake:`
    spelling is reported with its text and gets no edit.
  - The collector asks `retired_spelling(road, spelling)` over the merged
    catalog, which the loader builds in memory, because `nros fix` runs
    before configure and may run on a fresh clone. It never reads
    `<build>/nros/knobs/catalog.toml`.
  - Today it asks `nros_zephyr_build::retired_spelling_refusal`, which knows
    only core knobs. Until phase-581 lands the loader, that call stays the
    source, so switching to the merged catalog changes one call site.
  - One table-driven test runs every Appendix A rename through
    `nros fix`, so a rename added to the registry gets its fix with no
    hand-written case.

`app_rung`'s map-level refusal stays as the backstop for a road that reads
an image without the preflight.

### W5 — examples per board, from the CLI

- **Status:** OPEN
- **Owner:** nros 241, after its current batch
- **After:** phase-577 W7

`nros new --example <name> --board <key>` copies a board package's example
into a new project, and `nros board show <key>` lists the examples. The
examples come from the board package (RFC-0135 D7).

Acceptance: every in-tree board key lists at least one example, and each
copies and builds.

### W6 — `nros test`

- **Status:** OPEN
- **Owner:** nros 175, after its current batch
- **Needs:** a design note: how a test states what it expects (the probes'
  `probe-expect` output lines, or an exit code from the image), and where
  hardware runs fit (`nros bench`).

Run a project's test images on their target (host, emulator, or an attached
board), grading each by its console output, with one summary.

Acceptance: a scaffolded project's test image passes on the FreeRTOS QEMU
board and on the host, and a failing one is reported with its output.

### W7 — a board catalog page a user can filter

- **Status:** OPEN
- **Owner:** nros 139 AUO book, after phase-569 W2

Generate a page from the board registry, with filters for architecture, RTOS,
RMW, emulator and support status, linking each board to its book chapter and
examples. The generated support-levels table stays the source.

Acceptance: the page lists exactly the registry's boards (a gate compares
them), and works without a server.

### W8 — a shorter first build

- **Status:** OPEN — design ruled 2026-11-26: [RFC-0137](../design/0137-prebuilt-board-libraries.md) is Stable (input-digest keys, exact-match use, reproducibility; all four questions ruled as drafted). The items below run in the ruled order.
- **Owner:** nros 118, after its current batch

Ship prebuilt, per-board libraries in the store, the way the PX4 POSIX
archives already ship (phase-541), so a first build compiles only the user's
code. Measured on newslab-118 (RFC-0137 "Problem"): first builds of the
tutorial's `mps2-an385-freertos` talker take 64 s (Rust), 84 s (C) and 60 s
(C++) on 48 cores.

- **W8.a — one cargo run for the C image (ruling Q2, first).** The C talker
  builds both the `nros-c` and the `nros-cpp` image crates and links the C++
  one (also seen linking the TI image, phase-575 W3). Build one. Its own
  change, needing no prebuilt. Acceptance: the C talker's first build runs
  cargo once, with the measured before and after times stated.
  **DONE.** The change is #4235 (2100f094ce, 2026-11-18): `nros_c-static`
  became an interface over the `nros-cpp` archive, held by
  `check-c-image-one-cargo-run`. Measured 2026-11-26 on newslab-118 (48
  cores, load 2.6 at the start): fresh `nros new fr_c --board
  mps2-an385-freertos --template talker --lang c`, then `nros build`, with
  sccache off. The two sides ran back to back, one sample each, so read the
  saving as about 8 s, not a precise figure.

  | | wall | cargo runs | crates compiled |
  | --- | --- | --- | --- |
  | before (2100f094ce^) | 81.2 s | 2: `nros_c` 19.5 s, `nros_cpp` 21.5 s | 181 |
  | after (d92c369a14) | 73.0 s | 1: `nros_cpp` 22.9 s | 114 |

  Cargo time summed over runs fell from 41 s to 23 s, but the wall time fell
  only 8.2 s (10 %): ninja ran the two cargo runs side by side, so the
  removed run mostly overlapped the one that stayed. RFC-0137's "about 20 s"
  was the run's own length, not its share of the wall.
- **W8.b — reproducible pieces, then the weekly comparison (ruling Q3).**
  Measure how far zenoh-pico's and the Rust image crate's builds are from
  reproducible, and close the gap (`-ffile-prefix-map`,
  `--remap-path-prefix`, a fixed `SOURCE_DATE_EPOCH`, `ar D`). Then the
  RFC-0137 D3 job: weekly, it rebuilds a sample of published pieces from
  source under the same digest and compares the bytes. Advisory until green
  for a release, then gating the release. No prebuilt ships before this job
  exists.
  **Measured 2026-11-26 (the gap, before any fix).** The same C talker was
  built three times at d92c369a14, sccache off: twice from one checkout in
  two project directories, and once from a second checkout of the same
  commit. Comparing the outputs:

  | piece | another project dir | another checkout | what differs |
  | --- | --- | --- | --- |
  | zenoh-pico (`libzenohpico.a`) | identical | identical | nothing here. It is built from the store (`~/.nros/sources/zenoh-pico/<pin>`), not the checkout, and the `cc` crate already writes deterministic archive headers. Each object's `STT_FILE` symbol holds the store's absolute path (34 strings), so another store location (another user or host) will differ. |
  | Rust image crate (`libnros_cpp.a`) | identical | differs | panic locations hold the checkout's absolute path (76 strings), plus `~/.cargo/registry`, `~/.rustup` and the store path. Cargo's `-C metadata` hash comes from the absolute path of the path dependencies, so object names and 10 of 3566 exported symbols differ too. `--remap-path-prefix` fixes the strings but not the hash. |
  | lwIP (`liblwip.a`) | differs | differs | archive member headers only (mtime, uid/gid). All 40 objects are byte-identical, so `ar D` (CMake's deterministic archive flags) closes it. |
  | the linked image | identical | differs | follows from the Rust crate. |

  So a different project directory already reproduces. The gap is the
  build's absolute paths, and for the Rust crate that is more than strings:
  the metadata hash means a published piece must be built at a fixed path
  (or the same checkout-relative layout), not only remapped. Not yet
  measured: another host or user (`$HOME`, toolchain paths), and
  `SOURCE_DATE_EPOCH`, which nothing compared here reads. One trap met on
  the way: a checkout whose zenoh-pico submodule sits off its pin builds
  from that checkout as a local edit (the store's ladder), not from the
  store. Such a build is not of the digest's sources, and D1's input digest
  has to see the difference.
- **W8.c — input digests and the first two prebuilt pieces (ruling Q1).**
  zenoh-pico and nano-ros's Rust image crate: the input digest (D1), computed
  by the code that drives the build; exact-match use with the one-line reason
  on a miss (D2); `[prebuilt.<board>.<piece>]` rows produced by the release
  workflow for each board's default inputs (D3). Gates (D5): the
  default-image and changed-knob fixture, and the mutation-tested gate that
  every input reaching the compiler is in the digest. lwIP and the RTOS
  kernel come after these two.
- **W8.d — a prebuilt replaces its source download (ruling Q4).** `nros
  setup` skips the source of a piece it installs prebuilt; the source is
  fetched lazily when a build needs that piece from source.

Acceptance: from a clean store, the tutorial's first FreeRTOS QEMU build
completes in a stated time, measured by the probe.

### W9 — an editor integration

- **Status:** OPEN
- **Owner:** nros 243, after its current batch

First: `compile_commands.json` for every image kind (check which of the
POSIX and bare-metal C and C++ images lack one), and a `rust-project.json`
or rust-analyzer settings for Rust images, so an editor resolves symbols
without configuration. Later, and only with a user decision: a thin editor
extension for board choice, build, flash and monitor.

Acceptance: a scaffolded C, C++ and Rust project on each RTOS opens in an
editor with symbols resolved.

### W10 — compact logging for small boards

- **Status:** OPEN
- **Owner:** nros 142, after its current batch
- **Needs:** a design note against `nros-log` (RFC-0102): which log calls are
  interned, where the string table lives, and how `nros monitor` finds it.

Log arguments travel as indices into a string table in the ELF, formatted by
`nros monitor` on the host, on the pattern of defmt.

Acceptance: on the FreeRTOS QEMU board, an image logging through the compact
path is smaller and prints the same lines in `nros monitor`.

### W11 — a timeline of tasks and deadlines

- **Status:** OPEN
- **Owner:** nros 133, after its current batch
- **After:** phase-575 W5
- **Design:** RFC-0142, ruled 2026-11-26 (Stable): the existing executor
  trace stream plus job and budget events 28..31 and the
  `budget-overrun-runtime` rule; Perfetto as the viewer format; acceptance
  queried with `trace_processor_shell`, provisioned by the SDK. No code before
  phase-575 W5 lands, since it writes the new events' emitters.

Record executor and scheduling events (task start and end, budget overrun,
deadline miss) and export them in a format an existing viewer opens.

Acceptance: a QEMU run's trace opens in the chosen viewer and shows an
injected deadline miss.

## Order

W1, W3, W5 and W7 are small and independent, and the most visible: start
there. W2, W4 and W6 each need their design note first. W11 needs an
RFC; W8's, RFC-0137, is ruled. W9's first half is small; its second half
waits for a user decision.

## Related

- RFC-0129 (diagnostic codes), RFC-0133 and phase-569 (discoverability),
  RFC-0135 and phase-577 (board packages), RFC-0100 (sizing), RFC-0102
  (logging), phase-575 W5 (deadline monitors), phase-541 (prebuilt PX4
  archives).

## Issues homed here

| issue | why it belongs here |
| --- | --- |
