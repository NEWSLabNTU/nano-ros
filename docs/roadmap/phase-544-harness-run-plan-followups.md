# Phase 544 — The harness on `nros run --plan`: what phase-528 left

**Status (2026-11-09). IN PROGRESS: W1 done (issue 2172 resolved); W2 done (issue 2273 resolved); W3 open. Successor to [phase-528](archived/phase-528-run-on-host-and-emulator.md), which put the test harness on `nros run --plan` (W5) and was archived with three follow-ups open. Each work item here homes one of them.** Implements
[RFC-0118](../design/0118-target-execution-model.md) for the harness's
remaining QEMU and process-lifetime gaps.

Depends on:

- [phase-528](archived/phase-528-run-on-host-and-emulator.md) W3 to W5, for the
  runner packages, the verb and the harness on the plan.

---

## Why

Since phase-528 W5, every native and QEMU cell launches through `nros run
--plan`. Three things were left over. The harness still restates each QEMU
board's machine facts instead of reading the board descriptor. One emulator
cell that the safety work asked for was never built. And a verb killed with
SIGKILL leaves its image running.

## Work items

### W1 — The harness reads `[board.run.qemu]`

[Issue 2172](../issues/archived/2172-harness-mps-lines-restate-board-runner.md)'s
residue. `nros_tests::run::QemuBoot` is rendered by the same
`nros_qemu_ladder::plan_line` that `nros run` uses, but the harness's
`start_mps2_an385*`, `start_mps3_an536_*` and Zephyr `zephyr_qemu_boot`
still state `system`, `cpu`, `machine`, `icount`, `semihosting` and the NIC
model themselves. A descriptor edit therefore does not reach the harness.

- Build each table-shaped boot from the board descriptor's
  `[board.run.qemu]` (through the same reader the CLI's plan resolver uses,
  never a second parse). The harness adds only what is its own: the
  network (`user`, `socket,mcast`, a `hostfwd` list, a serial chardev) and
  passthrough words.
- Give the MPS3-AN536 descriptor its `-icount` (2172's second residue) and
  check it against a probe track or a harness cell.

Acceptance: no `cpu`/`machine` literal is left in `nros_tests::qemu` for a
board that has a descriptor, and a descriptor edit changes the harness's
`command_line()` (unit test).

**Status: DONE (2026-11-09).** `[board.run]` moved to
`nros_orchestration_ir::target_exec` with one mapping onto plan args,
`BoardRun::qemu_plan_args`, used by `nros run`'s plan, the cargo `runner`
(now rendered by `nros_qemu_ladder::plan_line` too) and the harness. The
harness reads a board through `nros_tests::qemu::DescriptorBoard`
(`MPS2_AN385`, `MPS3_AN536`) with `board_run_named`, and its launchers add
only their network, chardev, monitor or a measurement's fixed `-icount`. The
MPS3-AN536 descriptor states `icount = "shift=auto"` and `nic_model =
"lan9118"`. Guards: `check-harness-qemu-literals` rule 2 refuses a
descriptor board's machine or NIC model typed in `nros-tests/src`;
`board_run_reader_agrees_with_the_catalog` (CLI) holds the harness's reader
equal to the catalog's; `the_mps_boots_are_the_descriptors_and_follow_an_edit`
and `boards_on_one_qemu_machine_state_one_boot` (harness). Left as they are:
the NuttX and ThreadX boots, which already come from the one ladder
(`nros_qemu_ladder::Board`); `start_cortex_m3` (`lm3s6965evb`, no
descriptor); and the Zephyr boots, whose line is Zephyr's `board.cmake`
(no board states a `[board.run.qemu]`; the gate fires if one does).

### W2 — The violation channel's QEMU leg

[Issue 2273](../issues/archived/2273-violation-channel-qemu-leg.md): the same
`violation-cpp` workspace on a Zephyr QEMU target (mps2/an385, `-icount
shift=auto`), as a `fixtures.toml` row and a `violation_channel_e2e` cell in a
scheduled lane, asserting the single post-arming `max-latency-runtime`
verdict.

Acceptance: the cell runs in tier 2 and passes.

**Status: DONE (2026-11-09).** `violation-cpp` gained `[image.zephyr_mps2]`
(board `mps2-an385-zephyr`); row `workspace-zephyr-cortex-m-cpp-violation`
builds it, and `Workload::ViolationChannel`'s one cell,
(ZephyrQemuCortexM, Cpp, Zenoh, ViolationChannel, Workspace), is answered by
`violation_channel_e2e` under the descriptor's `-icount shift=auto`:
`contract violation #2: max-latency-runtime /handler/state measured=165
declared=50`, after RUN. Tier 2 pins the cell (`ci_lane::pinned_tier2_reqs`),
since its 1-wise cover never covers the workload; its counts stay 12 / 11.
Found on the way: a generated Zephyr entry linked a component archive after
`libkernel.a`, so a component calling `k_busy_wait` did not link on a real
board; `nano_ros_entry` now gives each component archive a `kernel` link
edge. Detail in the issue. That edge applies to every generated Zephyr
entry, so it was also built on the C and mixed rows in fresh build dirs: C
realtime (native_sim and the qemu_cortex_a53 SMP image) and mixed (native_sim)
link, and the C native_sim image delivers `/ctrl` to a ROS 2 subscriber. The
older build dirs of those two rows did not configure; that is
[issue 2374](../issues/2374-zephyr-cached-conf-file-freezes-board-fragment-discovery.md),
a cached `CONF_FILE` naming a fragment issue 2157 deleted, unrelated to the
edge. `ci_lane::tests::tier2_selects_every_pinned_cell` and
`lane_build_covers_run::the_tier2_build_carries_every_pinned_cell_leaf` hold
the pin: tier 2 runs the cell and builds its leaf (both fail with the pin
removed).

### W3 — A killed verb takes its image down

[Issue 2307](../issues/2307-killed-run-verb-orphans-its-image.md): the runner
dies with the verb (`PR_SET_PDEATHSIG`), but the image is the runner's child
and carries no such flag, so a SIGKILL to `nros run` orphans it. The harness
works around this in `run::stop`; a user's terminal does not.

Acceptance: a test that SIGKILLs `nros run` while a native image and a QEMU
guest run finds no survivor (a `/proc` scan, as phase-528 W4's Ctrl-C test does).

## Acceptance

| Work item | Industrial | Field solutions | Education |
| --- | --- | --- | --- |
| W1 | one place states how a board boots | — | — |
| W2 | the violation channel holds on an emulated core | — | — |
| W3 | no orphaned emulator after a killed run | — | a killed grading run leaves no process |

## Out of scope

Real boards (phase-529, phase-533) and the Renode and Wokwi twins
(phase-524).

## Issues homed here (2026-11-07)

| issue | why it belongs here |
| --- | --- |
| [#2350](../issues/archived/2350-run-dry-run-refuses-stale-model.md) | RESOLVED 2026-11-07. `nros run --dry-run` says it would configure, then refuses the stale SystemModel: a run-plan follow-up. |
