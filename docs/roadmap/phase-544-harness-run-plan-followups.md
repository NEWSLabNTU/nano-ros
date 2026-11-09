# Phase 544 — The harness on `nros run --plan`: what phase-528 left

**Status (2026-11-09). IN PROGRESS, W3 done — successor to [phase-528](archived/phase-528-run-on-host-and-emulator.md), which put the test harness on `nros run --plan` (W5) and was archived with three follow-ups open. Each work item here homes one of them.** Implements
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

[Issue 2172](../issues/2172-harness-mps-lines-restate-board-runner.md)'s
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

### W2 — The violation channel's QEMU leg

[Issue 2273](../issues/2273-violation-channel-qemu-leg.md): the same
`violation-cpp` workspace on a Zephyr QEMU target (mps2/an385, `-icount
shift=auto`), as a `fixtures.toml` row and a `violation_channel_e2e` cell in a
scheduled lane, asserting the single post-arming `max-latency-runtime`
verdict.

Acceptance: the cell runs in tier 2 and passes.

### W3 — A killed verb takes its image down

[Issue 2307](../issues/archived/2307-killed-run-verb-orphans-its-image.md): the runner
dies with the verb (`PR_SET_PDEATHSIG`), but the image is the runner's child
and carries no such flag, so a SIGKILL to `nros run` orphans it. The harness
works around this in `run::stop`; a user's terminal does not.

Acceptance: a test that SIGKILLs `nros run` while a native image and a QEMU
guest run finds no survivor (a `/proc` scan, as phase-528 W4's Ctrl-C test does).

**Done (2026-11-09).** Every first-party runner ties what it starts to itself
through `nros-runner-lifetime` (one PDEATHSIG hop, or a guarded process group
for a tool chain such as `west build -t run`, which one hop measurably does
not cover). `tests/killed_run_verb.rs` passes 4/4 in the ros2 box and fails
with the tie disabled. Detail in the issue's Resolution.

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
