# Phase 529 — Real boards: `nros flash`, `nros monitor`, `nros bench`, and human steps

**Status (2026-11-02). IN PROGRESS — W1 and W4 done; W2, W3, W5, W6 open.** Implements
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

**Landed (schema), 2026-11-01.**

- *Types.* `nros_orchestration_ir::target_exec` holds one definition for
  both levels. `FlashTable` has `method` or `command` (never both), `chip`
  and `prepare`. `ConsoleTable` has `kind`, `usb`, `baud` and `command`.
  `PrepareStep` is `say`, `wait_for`, `run`, or `say` paired with
  `wait_for`. `WaitFor` covers `usb:<vid>:<pid>` (four hex digits each),
  `port:`, `volume:` and `line:`, and any other kind is refused.
  `CommandTemplate` refuses a placeholder outside D5's list and
  `{tool:<name>}`, and an unclosed brace. Everything is checked when the file
  is read.
- *Board.* `[board.flash]` and `[board.console]` are in the descriptor schema
  (`board_descriptor.rs`). A board table must name a `method` or a `command`,
  and a board console its `kind`. The `method` word is checked for shape, not
  against a closed list. Flashers are runner packages (RFC-0119), so which
  words exist is answered by the installed runners when a plan resolves; a
  closed list in the file format would need a CLI release for every runner
  package. A board may state `[board.run]` and `[board.flash]` together. D1's
  "exactly one launcher or flasher" holds per target, and a board with an
  emulator twin is two targets. The Nucleo-F767ZI (Renode twin, flashed
  silicon) is the in-tree case.
- *Image.* `[image.<id>.flash]` and `[image.<id>.console]` take the same keys
  and may state any subset. They are parsed on both roads: the leaf road's
  `leaf_system::Network`, and the workspace road's `ImageBlock`, which takes
  the specific table whole over `[image_defaults]`'s. The key-by-key ladder
  over the board's is the RunPlan resolver's job.
- *Populated.* Only the Nucleo-F767ZI, the one board whose flash method is a
  stated fact (`west flash` in the Zephyr chapter): `method = "west"` and
  `[board.console] kind = "serial"` (USART6 over a USB-to-UART adapter, so no
  `usb`). The S32Z270 is flashed with the user's own S32 tooling ("nano-ros
  adds no flash step"), and the ESP32 package is a dormant QEMU board, so
  neither states one.

**Landed (execution), 2026-11-02.** W1 is done.

- *Where it runs.* `nros_cli_core::cmd::prepare::execute(steps, options,
  host, human, console)` runs a RunPlan's `prepare` steps. It lives in
  `nros-cli-core`, not `nros-extension`, because RFC-0119 D6 makes
  "`prepare` steps and `--no-prompt`" nros's job for every runner: a runner
  is asked to start or flash, never to talk to a person.
- *`say`.* Prints the instruction on stderr. With no terminal on stdin, or
  with `--no-prompt`, it stops the verb with exit 2.
- *`wait_for`.* Polls until its event holds, for the verb's `--timeout`,
  else 300 s. Timing out is also exit 2. Events are read under a `Host`
  root (sys, dev, proc), so tests use a fake tree:
  - `usb:` scans `sys/bus/usb/devices/*/idVendor|idProduct`. A DFU or
    mass-storage device is not a tty, so `nros_extension::usb`'s
    serial-port lookup cannot see it.
  - `port:` checks that the path exists.
  - `volume:` needs `dev/disk/by-label/<label>` (udev escaping) to resolve
    to a source listed in `proc/mounts`.
  - `line:` waits on a console that the caller passes in. Prepare runs
    before the target starts, so before launch it is refused, naming
    `--expect` instead; `nros flash --monitor` (W3) can open the console
    first.
- *`run` steps.* The plan carries them since the phase-527 W3 follow-up
  (`PrepareStep::Run`, and a typed `wait_for`). The executor runs the command
  through `sh -c`; a non-zero exit is exit 2. `--no-prompt` does not refuse a
  `run` step, because it needs no person.
- *Wiring.* No verb executes prepare yet. `nros run` has no prepare steps
  (`[board.run]` carries none) and no `--no-prompt` (RFC-0118 D2 lists that
  flag for `flash`). `nros flash` (W3) calls the executor.
- *Tests.* The acceptance test below, plus `usb:` and `volume:` against fake
  trees, `line:` with and without a console, and an unknown event kind.

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

**Landed (2026-11-02).**

- *`nros bench`.* It replaces phase-526's reserved stub in the user tier's
  "Run on a target" group. `bench` stays in `reserved::VERBS`, so a command
  package cannot claim it, and is listed in `reserved::IMPLEMENTED`.
- *`nros bench scan`* lists the attached USB serial devices through
  `nros_extension::usb` (sysfs, plus the stable `/dev/serial/by-id` links).
  It matches each device against the `[board.console] usb` of every board
  in the catalog, and writes `$NROS_HOME/bench.toml`, keyed by USB serial
  number. On a rescan it keeps an entry's name and every hand-added field
  (`probe`, `power_cycle`, anything else) and refreshes only the `port`.
  A device that matches two boards, or that has no serial number, is
  reported and not recorded. `--dry-run` writes nothing.
- *`nros bench list`* prints the file and whether each device is attached
  now.
- *Port resolution* is `cmd::bench::resolve_port`, RFC-0118 D6's ladder:
  1. `--port`, then `NROS_PORT` (the invocation rung);
  2. the bench file's entry, found on the bus by serial number;
  3. exactly one device matching the console's USB id.

  It refuses two candidates and a recorded device that is not attached,
  and lists what it found. Nothing on the ladder means no port, not a
  guess.
- *The RunPlan's `bench` rung* is `run_plan::bench_for`, which phase-528
  W1 left as an empty stub. It applies to flash targets only, with
  provenance `bench`: an emulator's console is never a device on the bus,
  so `nros run` and `nros build`'s next step pass an empty bench.
  `nros flash` (W3) calls `bench_for`.
- *Probes.* Debug probes that are not serial devices are not enumerated.
  `probe` is a hand-added field until W5 picks a probe runner.
- *No board states a USB id yet.* No in-tree board states `[board.console]
  usb` (the Nucleo's console is behind a USB-UART adapter), so a scan
  records nothing until a board, or the bench file by hand, names one.

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
- The bench file of the runner machine supplies ports and probes. It never
  decides whether a cell runs: a scope is a property of the lane, fixed
  before the run, so the same lane runs the same cells on every host
  (CLAUDE.md, issue 1758). Two parts:
  - **Scope.** Real boards are a host capability, `bench`, added to
    `nros_tests::lane_scope::Capability`. Every existing lane lists it in
    `NROS_TEST_UNCLAIMED` (through `nros_lane_unclaimed` and
    `CiLane::unclaimed`, the one place each lane's scope comes from), as it
    does `rt-sched`. A bench lane claims it and names the boards it covers,
    for example `just test bench boards=nucleo-f767zi-zephyr`; the board list
    is part of the lane's declaration, not something read from the machine.
    A hardware cell outside that scope (capability unclaimed, or its board
    not in the lane's list) is `nros_tests::lane_skip!`, decided before any
    probe.
  - **Precondition.** A cell in scope looks its board up in the bench file
    and resolves the port and probe (RFC-0118 D6). No bench entry, no
    attached device, or two candidates is `nros_tests::unmet!` naming the
    board and `nros bench scan`: a red, never a skip and never a pass.
  - Unset `NROS_TEST_UNCLAIMED` claims everything, as for every other
    capability, so a bare `cargo nextest` on a machine with no boards is red
    on the hardware cells. That is the honest answer for a run that asked
    for everything.
- `check-skip-budget` and `lane_scope`'s tests cover the new capability: a
  `lane_skip!` in a hardware cell must come from the scope predicate, and an
  unknown board name in a lane's list is a hard error, like an unknown
  `NROS_TEST_UNCLAIMED` token.

Acceptance:

- On the maintainer bench, `just test bench boards=<each phase-524 board>`
  runs each board's smoke cell through the verb, and it passes.
- Every existing lane deselects the hardware cells, counted as lane skips,
  identically on a machine with boards attached and on one without.
- In the bench lane, a board in the lane's list but unplugged is a red
  `[UNMET PRECONDITION]` that names the board.

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

## RFC-0120 (2026-11-03): what changes for this phase

[RFC-0120](../design/0120-nros-configure-one-resolver.md) adds `nros configure`. It covers what `nros sync` did, plus census, resolve and generated files. Every road calls it, and `nros build`/`nros run` call it implicitly when stale, like cargo. `nros sync` becomes its alias. A leaf that states no entities is sized from its census, and a host C/C++ compiler is required. Every host configure takes one pass, including a plain `west build`. The work is [phase-530](phase-530-nros-configure-single-pass.md). For this phase:

- **W3, `nros flash`'s "build if stale".** This goes through `nros configure` like `nros run`'s. A `west flash` after `nros build` runs no configure: the build directory is current.
- **The serial-first chapter's flash line.** It needs no `nros sync` before it. On Zephyr boards, a reader who uses `west build` / `west flash` directly gets the same tight sizing in one pass (phase-530 W4).
