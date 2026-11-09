---
id: 2375
title: "A killed `nros run` on the Renode road is not measured to leave no emulator behind"
status: open
type: test-gap
severity: low
area: [cli, runners, testing]
related: [phase-548, phase-544]
found: 2026-11-09
---

**Home: [phase-548](../roadmap/phase-548-three-languages-every-board.md) W7**,
with the Nucleo twin it needs. Split out of issue 2307 when
[phase-544](../roadmap/archived/phase-544-harness-run-plan-followups.md) was
archived.

## What is missing

[Issue 2307](archived/2307-killed-run-verb-orphans-its-image.md) made every
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
