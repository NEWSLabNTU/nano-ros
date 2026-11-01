# Phase 529 — Real boards: `nros flash`, `nros monitor`, `nros bench`, and human steps

**Status (2026-11-01). PROPOSED — no work item started.** Implements
[RFC-0118](../design/0118-target-execution-model.md) D2, D6 and D7 for real
silicon, and the flash and console runners of
[RFC-0119](../design/0119-cli-extension-packages.md) D8.

Depends on:

- [phase-528](phase-528-run-on-host-and-emulator.md) W1 and W4, for the plan,
  the resolver and the shared verb machinery;
- the boards [phase-524](phase-524-emulator-twins.md) buys, for the
  hardware acceptance.

phase-524 W5 (hardware runs) is executed with this phase's verbs.

---

## Why

Every board's first chapter opens over USB serial (RFC-0116 D5). Today
nothing in nros flashes a board or opens its console, so the chapter would
spell both by hand. Real boards also bring what emulators never do: ports
that move, probes identified by serial number, and a person who has to press
a button. RFC-0118 gives these a verb each, a per-machine file, and
`prepare` steps that print and wait instead of hanging.

## Work items

### W1 — `[board.flash]`, `[image.<id>.flash]` and `prepare`

- Add `[board.flash]` (method, chip and method facts, `prepare`) to the
  descriptor schema, and `[image.<id>.flash]` and `[image.<id>.console]` to
  `system.toml` (RFC-0118 D4, D5).
- Implement `prepare` steps: `say`, `wait_for` (`usb:`, `port:`, `volume:`,
  `line:`), `run`. With no terminal or with `--no-prompt`, a `say` step fails
  with exit 2 and prints the instruction.

Acceptance:

- A unit test drives a plan with a `say`/`wait_for = "port:<tmp path>"`
  pair. It creates the path after a delay and sees the verb continue. With
  `--no-prompt` it sees exit 2 and the instruction on stderr.

### W2 — Runner packages: `serial`, `espflash`, `west` (flash)

- `packages/runners/nros_runner_serial`: the console runner (baud, line
  ending, `--send`), the default console for `monitor` and `flash --monitor`.
- `packages/runners/nros_runner_espflash`: ESP32 flash and console, taking
  over the flashing path in `nros-tests`' ESP32 module.
- `nros_runner_west` gains `flash`: `west flash`, with `--runner` and
  arguments after `--` passed through to Zephyr's runner, and the board's
  default read from `runners.yaml`.

Acceptance:

- Each runner passes `nros dev runner check`.
- `nros flash --dry-run --why` resolves on every in-tree board that has
  `[board.flash]`.

### W3 — `nros flash` and `nros monitor`

- `nros flash`: build if stale, run `prepare`, flash, and with `--monitor`
  hand the same port to the console runner. Also `--expect`, `--timeout`,
  `--runner`, `--port`, `--probe`, `--plan`, `--dry-run`, `--why`, and
  arguments after `--`.
- `nros monitor`: console only, with `--port`, `--baud`, `--expect`,
  `--timeout`, `--send`.
- Both use phase-528 W4's capture, grading, router and teardown code; there
  is no second implementation.

Acceptance:

- On a bench board from phase-524, `nros flash --monitor --expect '<line>'`
  passes from a fresh terminal. The serial-first chapter's run section is
  that one line.
- `nros monitor --expect` passes against a board that is already running.

### W4 — `nros bench` and port resolution

- `nros bench scan` enumerates USB serial devices and probes, matches each
  board the workspace names by VID:PID, and writes
  `$NROS_HOME/bench.toml`, keyed by USB serial number with stable
  `/dev/serial/by-id/` paths.
- `nros bench list` prints the file and whether each device is present.
- Port resolution follows RFC-0118 D6: `--port`, `NROS_PORT`, the bench file,
  then exactly one VID:PID match. Otherwise refuse and list what was found.

Acceptance:

- With two matching devices attached and no bench entry, `nros flash`
  refuses and lists both.
- After `nros bench scan` it picks the recorded one.
- After a replug onto another `ttyACM` number it still picks the same
  board.

### W5 — The first probe runner, chosen by measurement

- Decide between `probe-rs` and `openocd` (and `picotool`/UF2 for the
  RP2040) by the boards phase-524 W1 keeps, and by what `s32z270-freertos`
  uses on its bench today. Write that runner package, and only that one.
- Record the choice and its measurement in RFC-0119 D8's table.

Acceptance:

- The chosen runner flashes its board under `nros flash --monitor --expect`.
- Its registry row's `last_run` is set from that run (RFC-0114 D10).

Decision step: if no kept board needs a probe runner (every pick flashes
over a bootloader `espflash` or `west` already reaches), this item closes
with that finding, and no runner is written.

### W6 — Hardware cells in the harness

- Hardware cells (phase-524 W5) run through `nros flash --monitor --plan`,
  as emulator cells run through `nros run --plan` (phase-528 W5).
- The bench file of the runner machine supplies ports and probes. A cell
  whose board is absent from it is skipped with `nros_tests::skip!` and the
  reason, never passed.

Acceptance: on the maintainer bench, each phase-524 board's smoke cell passes
through the verb, and the same cell on a machine without that board reports a
skip that names it.

## Acceptance

| Work item | Industrial | Field solutions | Education |
| --- | --- | --- | --- |
| W1 | a manual step is stated and fails loudly in CI | a technician's procedure is in the descriptor, not in a note | the lab sheet's "press BOOTSEL" is printed by the tool |
| W3 | flash and console are one command | one verb at the customer site | the grading command on hardware |
| W4 | benches with several boards stay unambiguous | a moved cable needs no edit | thirty lab machines, one file each |
| W6 | hardware evidence comes from the user's path | — | — |

## Out of scope

`nros debug` (RFC-0118 open question 1), runners for boards no phase has
bought, and power-switch hardware beyond the `run` step and `power_cycle`.
