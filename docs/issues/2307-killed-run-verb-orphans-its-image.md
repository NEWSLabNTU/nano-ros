---
id: 2307
title: "A SIGKILLed `nros run` orphans its image: the runner dies with the verb, the image it spawned does not"
status: open
type: bug
severity: medium
area: [cli, runners, testing]
related: [phase-544, phase-528]
found: 2026-11-04
---

**Home: [phase-544](../roadmap/phase-544-harness-run-plan-followups.md) W3.**

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
