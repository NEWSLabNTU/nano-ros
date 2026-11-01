# Phase 528 — `nros run` on host and emulator: RunPlan, runner packages, and the test harness on the same path

**Status (2026-11-02). IN PROGRESS — W1 and W2 landed; W3 to W5 not started.** Implements
[RFC-0118](../design/0118-target-execution-model.md) D1 to D5 and D8 for host
processes and emulators, and the host and emulator runners of
[RFC-0119](../design/0119-cli-extension-packages.md) D8.

Depends on:

- [phase-527](phase-527-cli-extension-packages.md) W1 to W3, for the kinds,
  the host build role and `nros-extension`;
- [phase-522](archived/phase-522-support-registry-primitives.md)'s generator, for W2 (landed; the phase is archived).

Carries [phase-525](phase-525-adoption-front-doors.md) W3: when W4 here is
accepted, phase-525 W3's acceptance is met by this phase's verb.

---

## Why

The launch of one board is spelled in four places today: the descriptor's
`runner` string, `nros-qemu-ladder`'s next-step line, `nros ws qemu-nic`, and
`nros_tests::qemu`. RFC-0118 puts it in one RunPlan with three consumers
(the build's next step, the verbs, the harness), and RFC-0119 puts the code
that executes it in runner packages. This phase does both for every target
that is not real silicon. That covers every target the tree's CI runs today.

## Work items

### W1 — RunPlan and the resolver

- The RunPlan types live in `nros-extension` (phase-527 W3). Write the
  resolver in `nros-cli-core`. It reads the RFC-0118 D5 ladder (invocation,
  image, bench, board, runner defaults) and records provenance per field.
- `nros build`'s next step is rendered from the plan, which replaces the
  `NextSteps` derivation in `build_next.rs`. The issue 2169 test keeps
  asserting that the printed line, the chapter's line and the probe's line
  are equal.

**Landed (2026-11-02).**

- *Resolver.* `nros_cli_core::cmd::run_plan::resolve(verb, inputs)` returns an
  `nros_extension::plan::RunPlan`. The ladder runs, highest rung first:
  `invocation` (`--port`, `--probe`, `--runner`, arguments after `--`),
  `image`, `bench`, `board`, `method`. The `image` rung is
  `[image.<id>.flash|console]` merged key by key over `[image_defaults]`'s
  when `system.toml` is read, on both roads. The `bench` rung reads nothing
  until phase-529 W4 writes `$NROS_HOME/bench.toml`. The `board` rung is
  `[board.run]`, `[board.run.qemu]`, `[board.flash]` and `[board.console]`.
  The `method` rung covers what the platform and triple imply: a host image,
  the `nros-qemu-ladder` NuttX/ThreadX boards, and `west` as a Zephyr board's
  flasher. Tables merge key by key across rungs. Each field records its rung
  in `provenance` (`"args.nic" = "board"`).
- *Target.* The verb picks it: `run` takes `[board.run]`, and `flash` takes
  `[board.flash]` (a Zephyr board falls back to `west`). `run` on real silicon
  refuses and names `nros flash --monitor`.
- *Runners* (RFC-0119 D8 names; the packages are W3's and phase-529's):
  - a host image uses `native`;
  - `fvp:` uses `fvp`;
  - any other emulator on a west build uses `west` (Zephyr's own board
    definition selects the emulator), and on any other build the emulator
    kind (`qemu`, `renode`, `wokwi`);
  - `flash` uses `command` for a template, or else the runner named by the
    method word.
- *Plan contents.* Artifacts come from the planner's recorded `BuildOutput`,
  and the link from the image's locator.
- *One QEMU renderer.* `nros_qemu_ladder::plan_line(&plan.args)` builds the
  QEMU line, both from the `[board.run.qemu]` facts and from the fixed
  ladder boards (`args.board`). It sits in the crate that already owns
  every QEMU board's line, has no dependencies, and takes a plain string
  map, so the W3 qemu runner can call it without depending on the CLI.
- *Next step.* `nros build`'s next step is rendered from the run and flash
  plans. Every issue-2169 row prints byte-identical text.
- *One changed line.* Zephyr `native_sim` is a host image, so its runner is
  `native` and its run line is now `<build>/zephyr/zephyr.exe`. It used to
  be `west build -d <build> -t run`, which runs that same executable. A
  Zephyr board with no `[board.run]` still gets its build's `run_<emu>`
  line, plus a note that `nros run` will need the table.
  `qemu-cortex-a53` now states `emulator = "qemu:virt"`.
- *`nros run`* is a user-tier verb (top-level, beside `nros build`). It
  takes `--dry-run`, `--why`, `--port`, `--json`, and arguments after `--`.
  It prints the plan as TOML, the RFC-0118 D3 human form, or as JSON with
  `--json`, the runner-contract form. An unresolved plan exits 3. Running
  without `--dry-run` exits 2 with a message saying it is phase-528 W4.

Acceptance:

- For every board in the 2169 test's rows, the next-step text is unchanged,
  or each changed row is listed in the commit with the reason.
- `nros run --dry-run --why` prints the plan with provenance.

### W2 — `[board.run]` and `[board.console]`, and the registry's `emulator`

- Add the two tables to the board descriptor schema, plus a runner-specific
  sub-table (`[board.run.qemu]`, read only by that runner).
- Migrate every `runner = "…"` string into them. Then delete the field, and
  refuse it with a message naming the tables.
- With phase-522: the generator reads `emulator` from the descriptor's
  `[board.run]`, and `board-support.toml` stops typing it (RFC-0118 D4,
  amending RFC-0114 D10). `check-board-registry` refuses a typed `emulator`
  that the descriptor does not state.

**Landed (2026-11-01).**

- *Schema.* `[board.run]` takes `emulator = "<kind>:<machine>"` (kind `qemu`,
  `fvp`, `renode`, `wokwi`) or `host = true`, never both; `[board.run.qemu]`
  takes `system`, `cpu`, `machine`, `icount`, `semihosting` and `nic_model` and
  needs a `qemu:` emulator; `[board.console]` takes `kind` (`stdio`, `serial`,
  `rtt`, `semihosting`) plus `usb` and `baud` for a serial console. Every table
  refuses unknown keys (`board_descriptor.rs`).
- *Migration.* The three MPS boards' cargo `runner` strings became
  `[board.run.qemu]`, rendered back in one fixed order. The FVP's
  `[board.zephyr] runner = "armfvp"` became `emulator = "fvp:FVP_BaseR_AEMv8R"`.
  Zephyr never read that key, because its own board definition selects
  `armfvp`. `NROS_BOARD_RUNNER` is now derived from the emulator kind, and its
  one reader, `nano_ros_entry()`'s "an embedded board was imported" test, sees
  the same value. The other emulated boards (NuttX ×2, ThreadX riscv64, ESP32,
  the Zephyr MPS2 board, the Nucleo's Renode twin) state `emulator` only, and the
  host boards state `host = true`. Both old spellings are refused naming
  the new tables, by the catalog and by `check-board-cargo-config-shape`. `nros
  new --platform baremetal` no longer writes a `.cargo/config.toml` (the bare-metal
  chapter already said it does not). `nros board new` emits the tables.
- *Launch readers.* `nros build`'s next step reads `[board.run.qemu]`. Every
  board's line is byte-identical, and the issue-2169 test is unchanged and green.
  The generated `nros-cargo.toml` gets the same line as the triple's cargo
  `runner`, so `cargo run --config …`, which the book documents, still boots a
  QEMU board. The NuttX and ThreadX riscv64 lines stay in `nros-qemu-ladder`:
  their virtio-mmio NIC varies per harness pair and does not fit the table's
  `-nic user` shape, so the qemu runner (W3) takes them.
- *Registry.* `board-support.toml` types no `emulator`. The generator reads it
  from the row's descriptor, found through `board_keys` or the package
  directory. `check-board-registry` refuses a typed one where an in-tree
  descriptor exists (duplicate or not), and refuses `board_keys` that span two
  descriptors. Its negative controls run on the normal path. The generated
  views are byte-identical, the data hash included, because the derived data
  equals what was typed.

Acceptance:

- No descriptor carries `runner =`.
- The registry's generated views are byte-identical before and after the
  move.
- `check-board-registry` is green.

### W3 — Runner packages: `native`, `qemu`, `fvp`, `west` (run), `command`

- `packages/runners/nros_runner_native`: host processes, including Zephyr
  `native_sim`'s `zephyr.exe`, with `--seed` handling the harness needs for
  paired native_sim runs.
- `packages/runners/nros_runner_qemu`: every QEMU board.
  - Absorbs the machine arguments from `[board.run.qemu]`, the NIC choice now
    in `nros ws qemu-nic` (which becomes a hidden alias of
    `nros run --dry-run` output), icount, and semihosting.
  - Absorbs the ESP32-on-QEMU flash-image step.
  - QEMU itself resolves through the SDK store, never `PATH`.
- `packages/runners/nros_runner_fvp`: the FVP case now in `build_next.rs`.
- `packages/runners/nros_runner_west` (run only here): `west build -t run`
  for Zephyr emulator boards.
- `packages/runners/nros_runner_command`: RFC-0118 D5 command templates.

Acceptance:

- Every in-tree board whose registry row says `runs = "host"` or
  `runs = "emulator"` resolves to one of these runners
  (`check-board-runner-resolves`, RFC-0119 D10).
- `nros-qemu-ladder` keeps only what the build needs, with no launch line.

### W4 — The `nros run` verb

- Implement RFC-0118 D2 for host and emulator targets:
  - build if stale, including the runner package;
  - `--expect`, `--expect-regex`, `--timeout`;
  - `--router auto|none|<locator>`, through the existing router resolution.
    An XRCE image's agent starts through the same switch;
  - `--prefix`, and arguments after `--`;
  - `--plan`, `--dry-run`, `--why`;
  - the D2 exit codes;
  - `console.log` capture and process-group teardown.
- On a hardware image (registry `runs = "hardware"`, or a board with only
  `[board.flash]`), refuse and name `nros flash --monitor`.

Acceptance:

- `nros run --expect 'Publishing: 0'` passes on the mps2-an385 QEMU talker,
  the Linux talker and the Zephyr `native_sim` talker.
- Ctrl-C leaves no QEMU, router or agent process (asserted by process-group
  scan in the test).
- phase-525 W3's acceptance is met and that work item is marked done by
  reference.

### W5 — The test harness goes through `nros run --plan`

- `nros-tests` writes a plan per cell and runs `nros run --plan <file>`,
  reading console output from the verb's stdout and verdicts from its exit
  code. This retires `nros_tests::qemu`'s command builder and the native
  spawn sites, and the ESP32-on-QEMU launcher.
- Keep the harness's own concerns where they are: port leases, ROS domain
  ids, DDS isolation (`dds_isolation::apply_to_command` applies to the
  `nros run` process), fixture freshness. The plan carries them as
  environment and arguments; the verb adds nothing of its own.
- Do it in two steps so a regression is attributable. First, native cells
  (`nros_runner_native`). Then QEMU cells (`nros_runner_qemu`). Each step
  runs the tier-1 lane, and the QEMU step runs tier 2, before and after.

Acceptance:

- Tier 1 and tier 2 have the same pass set before and after each step, run
  on freshly built fixtures (CLAUDE.md, fixture mtimes).
- Per-cell wall time moves by no more than 5 %, measured over the tier-2
  lane.
- No `qemu-system-` literal is left in `packages/testing/nros-tests/src/`
  outside comments (gate).

Decision step: if the verb's overhead breaks the 5 % bound on short cells,
fix the cost in the verb (lazy resolution, no rebuild check under `--plan`),
never by giving the harness a private path.

## Acceptance

| Work item | Industrial | Field solutions | Education |
| --- | --- | --- | --- |
| W1 | the printed command is the executed command | — | the chapter's line is the grading line |
| W2 | one place states how a board runs | — | — |
| W3 | an emulator twin is a package, not a special case | — | Renode and Wokwi twins land as runners |
| W4 | one smoke command per image | one verb to build and watch | the grading command |
| W5 | CI tests what users run | — | — |

## Out of scope

Real boards: flashing, the serial console, the bench file and port
resolution (phase-529). The Renode runner (written with phase-524's
Nucleo-F767ZI twin, against this phase's contract), and `nros debug`.
