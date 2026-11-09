---
id: 2307
title: "A SIGKILLed `nros run` orphans its image: the runner dies with the verb, the image it spawned does not"
status: resolved
type: bug
severity: medium
area: [cli, runners, testing]
related: [phase-544, phase-528]
found: 2026-11-04
resolved: 2026-11-09
resolved_in: "phase-544 W3: every first-party runner ties what it starts to itself through the new nros-runner-lifetime crate (one PDEATHSIG hop, or a guarded process group for a tool chain)"
---

**Home: [phase-544](../../roadmap/archived/phase-544-harness-run-plan-followups.md) W3.**

## What

`nros run` starts its runner with `PR_SET_PDEATHSIG`, so the runner dies when
the verb does. The image (a host process, or QEMU for an emulator target) is
the RUNNER's child, and nothing sets the flag on it:
`nros_extension::runner::exec` cannot, because the crate is
`#![forbid(unsafe_code)]`. So a SIGKILL to the verb, from anywhere,
leaves the image running and reparented to init.

Ctrl-C, SIGTERM and SIGHUP are fine: the verb turns them into a teardown of
its process group (phase-528 W4, asserted by a `/proc` scan).

## Where it shows

- The test harness avoided it in phase-528 W5. `nros_tests::run::stop`
  SIGKILLs the groups under the verb first, then the verb, and a dying test
  binary sends the verb SIGTERM (`set_orphan_group_suicide`). The harness is
  covered; any other caller that SIGKILLs the verb is not.
- A QEMU guest is the expensive case: an orphaned QEMU keeps its CPU and
  its slirp ports.

## What would close it

One of the two the phase-528 W5 record names:

- the runner sets PDEATHSIG on the image through an allowed API: a small
  unsafe-allowed helper crate, or the runner doing it in its own crate, as
  `nros_runner_serial` does;
- the verb keeps the image in a group the caller can kill, as the harness
  does, extended to every caller.

Acceptance is phase-544 W3's: SIGKILL `nros run` while a native image and a
QEMU guest run, and a `/proc` scan finds no survivor.

## Resolution (2026-11-09)

Fix (a), in one place: a new crate, `packages/runners/nros-runner-lifetime`,
the only runner crate besides `nros_runner_serial` that may use `unsafe`
(`nros-extension` still forbids it). It has two calls, and every
first-party runner that starts a process uses one of them:

- `die_with_runner(cmd)`: one hop. `PR_SET_PDEATHSIG(SIGKILL)` is set in
  `pre_exec`, and `getppid()` is re-read after `prctl`, so a child whose
  runner died between fork and prctl does not exec. PDEATHSIG fires when the
  forking THREAD exits; every runner spawns from its main thread, and the
  crate docs say so. Off Linux it is a no-op. Used by `qemu` (both spawns)
  and `native` without a prefix.
- `Chain` / `exec_chain`: any depth. The tool and everything under it join a
  process group led by a `/bin/sh` guard that holds the read end of a pipe
  only the runner can write. When the runner goes, by any signal, the pipe
  closes and the guard stops its group (SIGTERM, 1 s, SIGKILL); when the
  runner finishes normally it says so and the guard SIGKILLs leftovers. The
  first hop also gets `die_with_runner`. Works on any Unix. Used by `west`
  (run and flash), `fvp`, `renode`, `command`, `serial`'s console command,
  and `native` with `--prefix` (`gdb --args` forks the image).

The same `getppid()` re-check went into the verb's own spawn
(`target_session.rs`) and the harness's two `pre_exec` hooks
(`nros_tests::process`); `set_orphan_group_suicide` compared with pid 1,
which is wrong under a subreaper, and now compares with the parent's pid.

Sweep: `git grep -n 'Command::new' -- 'packages/runners/*/src/main.rs'` (every
spawn site goes through the crate) and
`git grep -n 'PR_SET_PDEATHSIG' -- '*.rs'` (every hook re-checks the parent).

## Verified (2026-11-09, ros2 box)

- The west road, measured: `west` (stand-in that spawns `cmake --build`,
  as west does) -> cmake -> ninja -> sh -> `qemu-system-arm`. With one
  PDEATHSIG hop on west only, QEMU SURVIVED a SIGKILL of its parent. Under
  `nros-runner-west` (chain), a SIGKILL of the runner left no QEMU.
- `tests/killed_run_verb.rs`: SIGKILL to the verb's pid alone while a native
  talker runs (direct, and under the `stdbuf` prefix) and while an
  mps2-an385 QEMU guest runs; a `/proc` scan (pid plus start time) finds no
  survivor within 10 s. 4/4 pass. A fourth case runs the harness's
  `run::stop`, which stays (a runner from another package may tie nothing).
- Negative control: with both calls disabled, the three SIGKILL cases fail
  naming the survivor (`talker`, `qemu-system-arm`) and `run::stop` still
  passes.
- `nros-runner-lifetime` unit tests: a tied child dies when its runner is
  SIGKILLed (fails with the tie disabled), and a guard stops a chain on
  close and on drop.

## Open after this fix

- **Renode is not measured.** The `renode` road takes the chain (its
  launcher may be a script), but no Renode run was SIGKILLed. Measure it
  with the Nucleo twin before claiming it. Carried as
  [issue 2375](../2375-renode-chain-lifetime-unmeasured.md), homed in phase-548 W7.
- **A verb that returns while its chain is still stopping.** On Ctrl-C a
  chained tool (`west`, `fvp`, `renode`, `command`) stops through its guard,
  so the verb can return about 1 s before the chain is gone, while it may
  still hold ports, which is the class issue 2315 hit. Manager ruling
  (2026-11-09): the verb waits until the guard reports the chain gone,
  bounded at about 2-3 s, then SIGKILLs the group and says so.
  **Done** (PR #3356):
  - Arming a chain prints `nros-runner: chain <pgid>` on the runner's
    stderr (`nros_extension::console::report_chain`). nros records it and
    does not forward it.
  - After stopping its own group, `ProcessGroup::teardown` waits up to
    `CHAIN_GRACE` (2.5 s) for each recorded chain group to empty. If one is
    left after that, it SIGKILLs the group and prints
    `nros: process chain <pgid> was still running ...; killed it (SIGKILL)`.
  - Measured with the `command` runner and a chain that ignores SIGTERM:
    the SIGTERMed verb took 1.06 s and left 0 chain processes.
  - Tests: `target_session::tests::teardown_waits_for_a_chain_that_is_stopping`
    and `teardown_kills_a_chain_that_does_not_stop`. Each goes red under
    both a zero-grace mutation and a skip-the-wait mutation.
