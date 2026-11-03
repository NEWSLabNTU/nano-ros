# Phase 528 — `nros run` on host and emulator: RunPlan, runner packages, and the test harness on the same path

**Status (2026-11-03). IN PROGRESS — W1, W2 and W3 landed (W3 with four items carried, and the ESP32 one moot); W4 done for host, QEMU and `native_sim` images, its serial-talker acceptance unblocked by issue 2231's fix; W5 not landed.** Implements
[RFC-0118](../design/0118-target-execution-model.md) D1 to D5 and D8 for host
processes and emulators, and the host and emulator runners of
[RFC-0119](../design/0119-cli-extension-packages.md) D8.

Depends on:

- [phase-527](phase-527-cli-extension-packages.md) W1 to W3, for the kinds,
  the host build role and `nros-extension`;
- [phase-522](archived/phase-522-support-registry-primitives.md)'s generator, for W2 (landed; the phase is archived).

Carries [phase-525](archived/phase-525-adoption-front-doors.md) W3: when W4 here is
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

**Follow-ups (2026-11-03).**

- *The emulator's NIC from the image (done).* `[image.<id>.run] nic` reaches
  the plan. The resolver's image rung now carries the image's resolved
  network (the same `NROS_IMAGE_NET` text the build bakes), and a QEMU board
  with `[board.run.qemu] nic_model` gets `args.nic` from
  `nros_board_common::image_net::qemu_nic` over the primary interface: the
  attach the image states (SLIRP, bridge or TAP), the board's model and the
  MAC the firmware was built with. Provenance is `image`. An image on a
  physical-only network refuses rather than booting on no network. An image
  with no network keeps the board's `user,model=<model>`, so the 2169 rows are
  unchanged. `nros ws qemu-nic` and the plan now compute the value through the
  same function.
- *A console `command` on the plan (done).* `nros_extension::plan::Console`
  gains `command`, an optional template that attaches the console instead of
  the runner's own console code. The JSON change is additive. The resolver
  carries `[board.console]` / `[image.<id>.console] command` key by key, with
  provenance `console.command`. `nros-cli-core/tests/console_wire_agreement.rs`
  holds the table and the plan console equal: every table key either crosses
  under the same name and JSON form, or has a listed home (`usb` becomes
  `args.usb`), and the plan's only key with no table is the resolved `port`.
- *Hardware detection from the registry (done, as split (a) and (b) below;
  see "How a board runs has one source" after the acceptance list).* What
  decided host, emulator or hardware in `resolve_run` is the descriptor:
  `[board.run] host`/`emulator`, else `[board.flash]` (refused, naming
  `nros flash --monitor`), else a method guess (no triple and not Zephyr means
  host). The registry cannot replace that as it stands. Its `runs` is
  evidence (RFC-0114 D10), not capability: `mps3-an536-freertos` and
  `esp32-qemu` say `runs = "none"` while their descriptors state a QEMU
  emulator, and the Nucleo says `runs = "emulator"` with `hardware` set. A
  board outside the tree has no row at all. RFC-0118 D4 assigns "how the
  board is launched" to the descriptor and "whether it was verified" to the
  registry. Proposed split: (a) retire the method guess, so a board is
  launchable only through `[board.run]` (every in-tree host board states
  `host = true` since W2); (b) extend `check-board-registry` so a row's `runs`
  must agree with its descriptor (`host` needs `host = true`, `emulator` needs
  a matching `emulator`, `hardware` needs `[board.flash]`); (c) let W4's
  hardware refusal keep reading the descriptor. Doing what the item literally
  says would mean amending RFC-0118 D4.

Acceptance:

- For every board in the 2169 test's rows, the next-step text is unchanged,
  or each changed row is listed in the commit with the reason.
- `nros run --dry-run --why` prints the plan with provenance.

**How a board runs has one source (2026-11-03; W1 follow-up 3, done).** The
decision taken: the descriptor holds launch facts and the registry holds
evidence (RFC-0118 D4). The registry therefore cross-checks against the
descriptor and is not read for launch facts.

- *The method guess is retired.* `resolve_run` no longer assumes "no triple
  and not Zephyr, so a host process". A board that states neither
  `[board.run]` nor `[board.flash]` is refused (exit 3), and the refusal
  names both tables and the `host = true` / `emulator = "<kind>:<machine>"`
  forms. Survey before the change: every tracked in-tree descriptor that runs
  states `[board.run]`. The one with neither table is `s32z270-freertos`,
  which its own descriptor says has never run anywhere. The
  `nros-cli-core/tests/fixtures/board-workspace` descriptors that lack the
  table feed only the host-platform vocabulary check, never a RunPlan.
- *`check-board-registry` cross-checks `runs`* (`runs_descriptor_errors` in
  `scripts/lib/board_registry.py`, with negative controls in its self-test).
  `runs = "host"` needs `[board.run] host = true`, `runs = "emulator"` needs
  a `[board.run] emulator`, and `runs = "hardware"` needs `[board.flash]`.
  The converse is not checked: a descriptor may state a launch nobody has
  verified yet. `mps3-an536-freertos` is that case. Its descriptor states
  `qemu:mps3-an536`, and its row stays `runs = "none"` because no runtime
  cell or probe track has shown it boot. The existing vocabulary already
  says "stated, unverified": status "Builds", with the machine shown as a
  twin. No evidence was added. `esp32-qemu` left the tree with phase-521 W6.
  Every row that runs resolves to a descriptor and agrees with it. The
  external `s32k344-safety-island` row has no in-tree descriptor and is not
  checked.

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

**Landed (2026-11-03), with the items below carried.**

- *Packages.* `packages/runners/nros_runner_{native,qemu,fvp,west,command}`,
  members of the runners workspace beside `nros-extension`. Each is a runner
  package of the user shape: a `package.xml` announcing
  `<nano_ros_provides kind="runner" name="…"/>`, an `nros-runner.toml`
  (`protocol`, `verbs`, `needs`; `bin` derived from the one bin target), and
  an executable that speaks RFC-0119 D6 protocol 1 unchanged. A test in each
  holds its `describe` reply equal to its descriptor. `just setup-cli` builds
  them, and `cli-tests` tests and lints them.
- *SDK helper.* `nros_extension::runner` (additive, no protocol change): the
  `main` every runner shares (`describe`, plan reading, a refused undeclared
  verb or a plan written for another verb), `exec` (inherited stdio; exit 0,
  1 or 2) and `exec_tool` (a failing flasher is the runner's failure),
  `split_words` for command templates, and `descriptor_agrees`.
- *native.* Executes the plan's `elf` with the passthrough arguments. A
  native_sim `zephyr.exe` gets `--seed=<n>` (the plan's `args.seed`, else one
  from the clock and the pid) and its arguments after `-testargs`; `args.seed`
  on any other host image is refused.
- *qemu.* Renders the line with `nros_qemu_ladder::plan_line`, the renderer
  the printed next step uses, and executes `plan.tools["qemu-system-<arch>"]`.
  It never searches `PATH`; a plan without the tool is refused, naming
  `nros setup --tool qemu` and `QEMU_SYSTEM_<ARCH>`. nros fills the tool:
  `run_plan::attach_tools` resolves it through the QEMU ladder (a store
  target) or, for a target the store does not build, the host's own by the
  ladder's policy, and records `tools.<exe> = method`. `nros run --dry-run`
  calls it.
- *west* (run). `west build -d <build_dir> -t run_<emulator>`, the emulator
  being the plan's kind and the target list read from the build's own
  `build.ninja`. An unconfigured build gets `-t run`; a build that declares
  no target for the kind is refused, naming the ones it has. Arguments
  after `--` are refused (`west build --` hands them to CMake).
- *fvp.* The FVP case of `build_next.rs` as a runner: west's `run_armfvp`
  target, with `ARMFVP_BIN_PATH` taken from the plan's tool for the model
  when it carries one. It shares the west lib's target lookup.
- *command.* Splits `args.command` into words, then expands each from the
  plan, so a value with spaces stays one argument; unknown placeholders are
  refused. A failing `flash` command exits 2, a failing `run` command 1.
- *Gate.* `check-board-runner-resolves` is
  `nros-cli-core/tests/board_runner_resolves.rs`: for every registry row with
  `runs = "host"` or `"emulator"` it resolves the run plan through
  `run_plan::resolve`, resolves the plan's runner through the provider scan
  of the nano-ros tree, and requires the provider's package to be the one
  the plan names, to implement `run`, and to need no placeholder the plan
  leaves empty (and, for `qemu`, args that render a line). All 13 rows
  resolve. It runs inside `cli-tests`; `just check board-runner-resolves`
  runs it alone. The runner lookup is one private helper, to be swapped for
  the shared provider-resolution function (RFC-0119 D4 overrides) when that
  lands. `check-provider-announcements`'s `runner` waiver is gone, and its
  stale-waiver control now runs on whichever family is still waived.

Carried:

- *ESP32 on QEMU.* Not absorbed, and now moot: phase-521 W6 retired the
  ESP32-C3 board, so no in-tree board has the flash-image step to absorb.
- *`nros ws qemu-nic` as a hidden alias.* The image `nic` now reaches the
  plan (the W1 follow-up), so the alias can print it; turning the verb into
  that alias is not done here, since `nros run`'s own surface is W4's.
- *`nros-qemu-ladder` with no launch line.* `board_line` is still what the
  test harness launches with; it leaves when W5 moves the harness onto
  `nros run --plan`.
- The qemu runner implements `run` only. `console` (RFC-0119 D8) arrives
  with `nros monitor` (phase-529 W3).
- The Pico W's Wokwi twin resolves to the `west` runner, which refuses it at
  run time: Zephyr declares no Wokwi run target. A `wokwi` runner (or a
  resolver rule naming one) is the next step for that board.

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

**Done for host, QEMU and `native_sim` images (2026-11-03).** The
serial-talker acceptance was blocked by issue 2231, which reproduced without
`nros run`; it passes since that fix (a `zpico-serial` read timeout that
counted waits instead of measuring time). The runs are recorded after the
list below.

- *Executor.* `cmd/run_exec.rs`. Unless `--plan` names a plan file, which runs
  as written, `nros run` resolves the plan (W1), builds the image through
  `nros build` with the build's output on stderr (stdout stays the console),
  finds the plan's runner on the provider search path, checks its `describe`
  reply (protocol and verb), writes `<ws>/build/nros/run/<image>/plan.json`
  and runs `<runner> run --plan plan.json`. Where the executable is: a
  workspace runner package's host-tool directory (`nros build` builds it, and
  an unbuilt one is refused naming `nros build --packages-select`), a nano-ros
  checkout's `packages/runners/target/release` (built by `just setup-cli`), or
  a released package's `bin/`.
- *Session.* `target_session.rs` holds the process group, console capture,
  grading and teardown, and has no verb in it, so `nros flash --monitor` and
  `nros monitor` (phase-529 W3) reuse it. Every process joins one new process
  group, and on Linux gets `PR_SET_PDEATHSIG`. Ctrl-C, SIGTERM and SIGHUP set
  a flag, and the verb stops the group (SIGTERM, 2 s, then SIGKILL). The
  console is copied unmodified to stdout and `console.log`. `--expect`
  (substring) and `--expect-regex` grade it line by line, and an unterminated
  tail counts as a line. On an unmet expectation the last ten console lines
  are repeated on stderr.
- *Exit codes* (RFC-0118 D2): 0 matched or ran clean; 1 the target failed,
  ended without the match, or `--expect` timed out; 2 the runner, router or
  build failed; 3 the plan did not resolve. A `--timeout` with no `--expect`
  is a clean stop (0). Ctrl-C is 130, the shell's SIGINT code, because an
  interrupt is not a verdict about the target.
- *Router* (`--router auto|none|<locator>`). With `auto`, a zenoh link gets
  ROS's `rmw_zenohd` (the CLI's existing resolver, configured through
  `ZENOH_CONFIG_OVERRIDE`, in its prefix's environment and with its paired
  `libzenohc`), and an XRCE link gets the store's pinned `MicroXRCEAgent`.
  Either starts only when the locator is this host's: loopback, an address
  that can be bound here, or `10.0.2.2` for an emulator on QEMU's user
  network, which then listens on `127.0.0.1`. A network router starts before
  the runner, and the verb waits until its TCP port accepts. A serial link
  starts its router or agent on the endpoint the runner reports on stderr
  (`nros-runner: link <pty>`), keeping the locator's `#baudrate=` options. A
  router or agent that exits early ends the run with exit 2, naming its log
  (`<run dir>/router.log`, `agent.log`). `--router <locator>` starts nothing
  and passes the locator to the target as `NROS_LOCATOR`.
- *Plan additions.* `link.rmw` is an optional field, added to the JSON
  without a protocol change: it records which RMW speaks over the link, which
  decides router or agent. A host zenoh image with no stated locator gets the
  zenoh default link, `tcp/127.0.0.1:7447`, or `NROS_LOCATOR` when that is
  set (provenance `method`). `--prefix` reaches the runner as `args.prefix`.
  All of this is applied before `--dry-run` prints, so the printed plan is
  the executed plan.
- *Hardware.* The W1 resolver already refuses a board with only
  `[board.flash]`, naming `nros flash --monitor` (exit 3).
- *Tests,* compiling nothing: `target_session` runs shell children
  (grading, the regex tail, timeout, link callback, a required member dying,
  and Ctrl-C with a grandchild and a second member, asserted by a `/proc`
  process-group scan). `run_exec` covers the router decision table, and
  covers the executor against a shell-script fake runner, both handed in
  directly and found as a workspace runner package (provider scan plus
  host-tool directory).
- *Wired to W3's runners.* The `qemu` runner gets its QEMU from the plan's
  `tools`, which `run_plan::attach_tools` fills before the plan is printed
  or executed. A plan's `console.command` is the console: `nros run` starts
  it in the session's group and grades its stdout, and the runner's stdout
  becomes a diagnostic. The native runner honours `args.prefix`. On a serial
  link the qemu runner gives the first UART a host pty (`-serial pty
  -monitor none`), reports it as `nros-runner: link <pty>` (QEMU 11 prints
  the pty on stdout, so both streams are read), and starts the guest paused,
  resuming it over QMP 2 s after the report so the router has opened the
  pty before the guest's first handshake.
- *Build selection.* The build-if-stale step passes `nros build` the
  selection the plan was resolved from (`--workspace`, `--nano-ros-path` and
  the image as the user named it), never the qualified
  `<bringup>:<image>`, which a Zephyr leaf's `nros build` refuses.

*Runs (2026-11-03, newslab-server-241, load 70 to 150 on 24 cores).* Wall
time is the whole verb, build included; the match time is from launch.

| Image | Result | Wall | Matched after |
| --- | --- | --- | --- |
| `examples/linux/rust/talker` | exit 0 | 33.1 s | 1.4 s |
| `examples/mps2-an385-baremetal/rust/talker` (QEMU, router on `tcp/127.0.0.1:10500` for the guest's `10.0.2.2`) | exit 0 | 7.1 s | 1.3 s |
| `examples/zephyr/rust/talker` (`native_sim`, west build) | exit 0 | 163.5 s | 3.3 s |
| `examples/mps2-an385-baremetal/rust/serial-talker` (before issue 2231's fix) | exit 1, `ConnectionFailed` | 12.8 s | — |
| `examples/mps2-an385-baremetal/rust/serial-talker` (after issue 2231's fix) | exit 0 | — | 3.1 s |

- The expectation used was `--expect 'Publishing:'`. These talkers print
  `Publishing: 'Hello World: <n>'`, so the literal `'Publishing: 0'` below
  matches none of them. The mps2-an385 talker's count starts at 1.
- The serial talker's link plumbing works: the runner reported the pty, and
  `rmw_zenohd` started listening on it. The session itself fails. It fails
  the same way in the test harness's own socat topology, with no `nros run`
  involved, so this is issue 2231 and not this verb. Resolved: the guest's
  serial read gave up after 500 early-returning waits, a few milliseconds
  under QEMU 11, and re-sent the link INIT the router had already answered.
- Ctrl-C on a real run: `nros run` on the mps2-an385 talker with a router,
  QEMU and the runner in its group was sent SIGINT after 25 s. It exited 130,
  QEMU logged `terminating on signal 15`, and the group had no member left.

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

## RFC-0120 (2026-11-03): what changes for this phase

[RFC-0120](../design/0120-nros-configure-one-resolver.md) adds `nros configure`. It covers what `nros sync` did, plus census, resolve and generated files. Every road calls it, and `nros build`/`nros run` call it implicitly when stale, like cargo. `nros sync` becomes its alias. A leaf that states no entities is sized from its census, and a host C/C++ compiler is required. Every host configure takes one pass, including a plain `west build`. The work is [phase-530](phase-530-nros-configure-single-pass.md). For this phase:

- **W4, "build if stale".** This is `nros build`, which now configures when its stamp is stale (RFC-0120 D3). Today the `--dry-run` path calls `plan_builds` directly; phase-530 W1 puts `configure::ensure_current` in front of it.
- **W3, the `west` run runner** (`west build -t run`). A host configure it triggers calls `nros configure` from the module hook (phase-530 W4). With a current stamp that is a no-op, so the runner adds no pass.
- **W5.** The harness's fixtures are configured by the fixture build. A current stamp makes every hook a no-op, so `nros run --plan` adds no configure.
