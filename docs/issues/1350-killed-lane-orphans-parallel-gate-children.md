---
id: 1350
title: "Killing a `just ci gate` lane leaves its `-P 32` gate fan-out running as
  an orphan — the wrapper dies, reads as success, and the load keeps building"
status: open
type: bug
area: ci, tooling
severity: medium
found: 2026-09-12
related: [issue-0616, issue-1280, phase-454]
---

## Measured, twice, independently

Two phase-454 agents stood their waves down on the same host and each found the
same thing: a `SIGTERM` to the lane did not take its parallel gate children with
it.

**Observation A.** `kill -TERM` on the shell wrapper killed the wrapper and left
`just check build` alive, with its `xargs -P 32` fan-out **still spawning fresh
`compile-check-fixtures.sh` children** — new `rustc`/`cc` processes appeared
**6 seconds after** the kill. Two rounds of killing named PIDs merely raced the
respawn. `kill -TERM -<pgid>` on the process group is what worked.

**Observation B.** A `ci gate` run reported `rc=124` / SIGTERM at the `gate`
recipe, and its already-spawned `check::build` **survived as an orphan**, still
running `run-gates-parallel.sh` at `-P 32` and driving fresh `cargo`/`rustc`/
`cc1` children. It was found only because newly-spawned PIDs kept reappearing
after each kill; a live leaf was traced up ten levels to the root (`bash …/gate`),
and `/proc/<pid>/cwd` confirmed it belonged to that worktree rather than a
sibling session.

Neither agent was looking for this. Both found it while cleaning up.

## Why it matters here specifically

The orphaned subtree consumes CPU **and disk** while attributed to no visible
task. During phase-454 this host ran up to seven concurrent waves, `/home` hit
**0 bytes free twice**, and several waves lost their `check::build` verdict to
`No space left on device`. Some of that load had no owner — a lane someone had
already stopped.

It also compounds two issues already open:

* **1280** — inherited paths aimed a worktree's build at another checkout, so an
  orphan can be writing into a *different* worktree's `target/` than the one
  whose agent thinks it stopped.
* **0616** — one cargo target dir with concurrent consumers; an invisible
  consumer is the worst version of that, because the contention has no
  attributable source.

## The failure is silent in the direction that looks fine

`kill -TERM` on the wrapper **returns success**. The operator sees a clean
stand-down; the machine keeps compiling. Nothing reports that a lane is still
running, and `just` has already printed its own exit.

## Adjacent trap, same session

`just` reports an **unknown recipe** as a failing exit code, indistinguishable
from a red gate unless the text is read. One agent invoked `just check build`
instead of `check::build` (the real step names are in `just/ci.just`), got a
non-zero rc, and nearly reported two gate failures that were `justfile does not
contain recipe` — *no verdict at all*. That is the "a gate that WORKS is not a
gate that RUNS" shape (issue 1226) one level out: here the gate never ran and the
exit code cannot say so.

Worth fixing in the same pass if the lane learns to distinguish its own failure
modes.

## What a fix has to decide

* Whether the lane installs a `trap` that kills its **process group** on
  INT/TERM, so stopping the wrapper stops the fan-out. `run-gates-parallel.sh`
  is the natural owner, since it is what creates the `-P 32` breadth.
* Whether `xargs -P` is the right primitive at all, or whether the fan-out should
  run under a job-control-aware runner that can be signalled as a unit.
* Whether a stale-lane detector is worth having: on a shared host, "is anything
  from a stopped lane still running under this worktree?" is answerable from
  `/proc/<pid>/cwd`, which is how both agents found it by hand.

## Reproduction

Start `just ci gate` in a worktree, wait for `check::build` to reach its
`run-gates-parallel.sh` fan-out, then `kill -TERM` the wrapper PID. Watch
`pgrep -f compile-check-fixtures` — children continue to spawn. `kill -TERM
-<pgid>` stops it.

Found during phase-454 stand-downs by two independent agents.
