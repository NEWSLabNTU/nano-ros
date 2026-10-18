---
id: 1350
title: "Killing a `just ci gate` lane leaves its `-P 32` gate fan-out running as
  an orphan — the wrapper dies, reads as success, and the load keeps building"
status: resolved
type: bug
area: ci, tooling
severity: medium
found: 2026-09-12
related: [issue-0616, issue-1280, phase-454, phase-505]
resolved_in: "phase-505 W3: lane runners own their fan-out as a process group (`nros_guard_lane`); gate `check-lane-stop-kills-fanout`"
---

**Home: [phase-505](../../roadmap/phase-505-ci-lanes-reach-and-infra-robustness.md)** — homed 2026-10-17; listed in that phase's "Issues homed here (2026-10-17)".

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

## Resolution (2026-10-19, phase-505 W3)

**Why the stop did not reach the fan-out, measured.** `just` 1.51 forwards a
signal to its DIRECT child only. A scratch recipe running
`bash -c "sleep 300; …" & wait` was sent SIGTERM through its `just`: `just`
reported `recipe was terminated by signal 15`, and the `bash -c` and its
`sleep` stayed alive. So stopping `just ci gate` kills the recipe shell and
nothing else. `run-lane-steps.sh`, the recipe shell's child, is reparented and
goes on into `check::build`, whose `run-gates-parallel.sh` keeps its `xargs -P`
fan-out running. That is Observation B exactly.

**Fix.** `scripts/build/subtree-guard.sh` gains `nros_guard_lane <name> cmd…`.
It is the lane version of `nros_guard_exec` and takes no lock, because two
lanes in one tree are legitimate. It does three things:

- The command runs in its own process group.
- A trap on INT/TERM/HUP kills that group, prints `NO VERDICT` and exits 128+N.
- A **sentinel inside the group** watches the owner and the owner's parent (the
  recipe shell that `just` kills). When either one goes away, the sentinel
  kills the group. That includes a SIGKILL of the owner, which no trap can
  catch.

Two owners use it: `run-gates-parallel.sh` (the `xargs` fan-out) and
`run-lane-steps.sh` (each lane step). Stopping `just ci gate`, its recipe
shell, the lane runner or the gate runner now stops the whole tree. A stopped
lane prints `ci <lane> STOPPED by SIG<x> during step k of n (<step>) — NO
VERDICT` and exits 128+N, never 0. The guarded command reads stdin from
`/dev/null`. The group is not the terminal's foreground group, so a step that
touched the tty would be stopped by SIGTTIN/SIGTTOU.

**The adjacent trap.** `run-lane-steps.sh` now runs `just --show` on every
step before it starts any. A step that is not a recipe makes the lane exit
**2** with `NO VERDICT — step <x> is not a recipe, so the lane did not run`.
The case the issue reports, a wrong recipe typed by hand, never goes through a
runner, so only `just`'s own message can report it. Inside a lane, the exit
code now tells it apart from a red gate.

**Gate: `just check lane-stop-kills-fanout`** (fast line, ~14 s). It drives the
real `run-gates-parallel.sh` and `run-lane-steps.sh` against a stub `just`
whose 40 gates are long sleeps at `-P 4`. It checks the process table twice,
2 s apart, because this bug showed itself as NEW gates appearing after the
kill. The cases:

- SIGTERM to the owner: rc 143, NO VERDICT, no survivor.
- SIGTERM to only the shell above the owner: no survivor, and the owner
  reports `STOPPED by SIGHUP`.
- SIGKILL of the owner: the sentinel fires, no survivor.
- SIGTERM to the lane runner mid-step: rc 143, and it names the step.
- An unknown step: rc 2, nothing ran.

**Negative control**, on every run: the parent-TERM case with
`NROS_GUARD_DISABLE=1` must leave survivors, and it does. That reproduces the
issue's orphan on demand. The gate also caught a bug in the first version of
the fix: the sentinel read its holder pid inside the `&`, so it exited at once,
and the gate reported 19 survivors in the parent-TERM case and 18 in the
owner-KILL case. The gate finds every process it signals by a per-run token
(its temp dir and a unique `sleep` duration), never by name.

Deliberately not done: a host-wide stale-lane detector. The owners now reap
their own groups, so leaving an orphan takes a SIGKILL of BOTH the owner and
its group, and the sentinel covers that too.

**Seen live during this change's own verification.** A `just ci gate` run of
this branch was stopped from outside mid-`check::build` (2026-10-19). The log
ends with `subtree-guard: caught SIGTERM — stopping the 'ci-gate' lane …` and
the same for `gates-build`, and a `/proc/*/cwd` scan of the worktree right
afterwards found no process left. That run also showed the handler being
entered twice when the outer group's kill delivered a second TERM, so the
handler now ignores further INT/TERM/HUP once it starts.
