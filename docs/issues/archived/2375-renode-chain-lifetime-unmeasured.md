---
id: 2375
title: "A killed `nros run` on the Renode road is not measured to leave no emulator behind"
status: resolved
type: test-gap
severity: low
area: [cli, runners, testing]
related: [phase-548, phase-544, phase-524]
found: 2026-11-09
resolved: 2026-11-15
resolved_in: "phase-524 W4c"
---

**Home: [phase-524](../../roadmap/phase-524-emulator-twins.md)**, which owns the Nucleo
Renode twin and its launcher. Rehomed 2026-11-15 from
[phase-548](../../roadmap/archived/phase-548-three-languages-every-board.md) W7 (archived). Split out of issue 2307 when
[phase-544](../../roadmap/archived/phase-544-harness-run-plan-followups.md) was
archived.

## What is missing

[Issue 2307](2307-killed-run-verb-orphans-its-image.md) made every
first-party runner tie what it starts to itself (`nros-runner-lifetime`).
`nros_runner_renode` starts Renode as a process chain, because its launcher
may be a script that starts the emulator further down. Since PR #3356 the verb
also waits up to 2.5 s for a chain to stop after a Ctrl-C.

Both behaviours were measured on the native, QEMU and `command` roads. Neither
was measured on the Renode road:

- a SIGKILL to `nros run` while a Renode twin runs;
- a Ctrl-C with the Renode chain still stopping.

`tests/killed_run_verb.rs` has no Renode case.

## Acceptance

- A `killed_run_verb` case for the Nucleo Renode twin (phase-548 W7's board)
  sends SIGKILL to the verb alone, and the `/proc` scan finds no survivor
  (Renode, its mono runtime, any launcher script).
- A Ctrl-C (SIGTERM to the verb) on the same twin returns only after the chain
  is gone, without the "killed it (SIGKILL)" line, or the case records why
  Renode needs the SIGKILL.
- The case runs in the lane that already runs the Renode twin, so it is not a
  new skip.

## Resolution (2026-11-15)

`tests/killed_run_verb.rs` has two Renode cases on the Nucleo-F767ZI twin.
Each boots the Nucleo C serial talker (`[image.nucleo_serial]`) through
`nros run --plan` and the real `renode` runner, with `nros_tests::run::renode_image`
writing the plan. The machine, link and console come from the board
descriptor's `[board.run]` / `[board.run.renode]`, so no board fact is
restated. Both cases are gated on `PlatformId::ZephyrNucleoF767zi`, the lane
that already runs the twin's smoke cell, so neither is a new skip.

- `sigkilled_verb_leaves_no_renode_twin`: SIGKILL to the verb alone. The
  recorded tree (`nros-runner-renode`, `renode`, and the chain guard `sh`) is
  gone within the 10 s teardown window.
- `sigtermed_verb_returns_after_the_renode_twin_is_gone`: SIGTERM (a Ctrl-C).
  The verb returns (status 130, 1.05 s after the signal) with every recorded
  process already gone, and without the verb's "killed it (SIGKILL)"
  fallback: the chain stops on the runner's own teardown, and Renode needs
  no SIGKILL.

Both cases passed in 3 of 3 runs.

**Negative control.** With the runner's `lifetime::Chain::join` removed, so
Renode is a plain child of the runner, the SIGKILL case fails with
`left processes running 10s later: [(…, "renode")]`. The SIGTERM case still
passes under that mutation, because the verb stops the runner's process
group and Renode is in it. The chain is what the SIGKILL case is about.

To build the image, a worktree needed a private Zephyr workspace. The shared
store workspace lacks `hal_stm32` (issue 2607), and the main checkout's
workspace has a damaged zephyr tree.
