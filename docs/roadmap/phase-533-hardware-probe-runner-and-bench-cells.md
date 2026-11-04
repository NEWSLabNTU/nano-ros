# Phase 533 — Hardware: the first probe runner and bench cells in the harness

**Status (2026-11-04). OPEN — W1 and W2 not started. Both wait on the boards phase-524 W5 puts on the maintainer bench.** Moved from
[phase-529](archived/phase-529-flash-monitor-bench.md) W5 and W6 (maintainer,
2026-11-04), so phase-529 could close on what runs without a bench.
Implements the probe-runner row of
[RFC-0119](../design/0119-cli-extension-packages.md) D8 and the hardware
cells of [RFC-0118](../design/0118-target-execution-model.md) D6.

Depends on:

- [phase-529](archived/phase-529-flash-monitor-bench.md) W1 to W4: `[board.flash]`,
  the `serial` and `west` runners, `nros flash`/`nros monitor`, and
  `nros bench`;
- [phase-528](phase-528-run-on-host-and-emulator.md) W5, for the harness
  route (`nros run --plan`) that W2 mirrors for hardware;
- the boards [phase-524](phase-524-emulator-twins.md) W5 puts on the bench.

---

## Why

Phase-529 gave real boards their verbs. Two items need the boards
themselves: choosing a probe runner is a measurement on the kept boards, and
hardware cells are only evidence when they run on a bench. Neither can be
done, or accepted, on a machine with no boards attached.

## Work items

### W1 — The first probe runner, chosen by measurement

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

### W2 — Hardware cells in the harness

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
| W1 | one probe tool, chosen by measurement | the field technician's probe is the one the tool drives | — |
| W2 | hardware evidence comes from the user's path | — | — |

## Out of scope

`nros debug` (RFC-0118 open question 1), runners for boards no phase has
bought, and power-switch hardware beyond the `run` step and `power_cycle`.
