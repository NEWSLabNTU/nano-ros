---
id: 853
title: "The subtree guard's SIGTERM path fails only on the GitHub runner — the
  'survivors' are ZOMBIES, because a container job's PID 1 (`tail -f /dev/null`)
  never reaps"
status: resolved
type: bug
area: testing
related: [issue-0762, phase-395]
---

> **Cause found and fixed in the working tree (2026-08-28); this doc flips to
> `resolved` when that lands.** Reproduced in the real CI image and in bare
> `ubuntu:22.04`, isolated to a single variable, fixed, and proved with a
> negative control. Details below.

## Problem

`check-subtree-guard` fails on every GitHub push and passes everywhere I can
reproduce. It blocks `pr-checks`, which means **no push gets a green PR run** —
the condition phase-395 W0.5 exists to remove.

```
FAIL: subtree survived SIGTERM to its launcher — 2 of ITS OWN process(es)
      still in pgid NNNN. This is the orphan bug.
```

## The cause: a zombie is not a survivor

The processes are real — the identity check (same pid AND same pgid, added in
W0.5) was right about that. What neither the test nor the guard checked is
whether they are still **processes**. They are not: they are zombies.

```
== before TERM (pgid=577, launcher=573) ==
  PID  PPID  PGID STAT COMMAND
  577   573   577 S    bash
  580   577   577 S    bash
  581   580   577 S    sleep
  582   580   577 S    sleep
== 6s after TERM to the launcher ==
  581     1   577 Z    sleep <defunct>
  582     1   577 Z    sleep <defunct>
== pid 1 ==
    1 tail
```

The guard did its job: the whole subtree took the signal and exited. But a
zombie **keeps its pid and its pgid**, keeps a `/proc/<pid>` directory, and stays
in `ps` output until its parent calls `wait()`. Here the parent — the launcher —
was itself killed, so the corpses were reparented to PID 1.

**A GitHub Actions `container:` job runs with `--entrypoint tail <image> -f
/dev/null` and no `--init`, so PID 1 is `tail`, which never calls `wait()`.**
The zombies are therefore *permanent*, and every state-blind `ps -eo pid=,pgid=`
in this code reads a fully-drained group as still alive. Nothing about the
image, the runner's CPU count, or PID recycling is involved.

That is also why every previous local attempt passed. `docker run <img> bash
script.sh` makes **bash** PID 1, and bash reaps adopted orphans as a side effect
of its own `waitpid` loop. The reproduction was one `--entrypoint` away the whole
time.

## Isolation — one variable, four runs

Same image (`ubuntu:22.04` + `procps`), same bash 5.1.16, same procps-ng 3.3.17,
same `root` user, in every row. Only PID 1 differs:

| PID 1 | reaps orphans? | result |
| --- | --- | --- |
| `bash` (`docker run … bash script.sh`) | yes | **PASS** |
| `docker-init` (`--init`, entrypoint `tail`) | yes | **PASS** |
| `tail -f /dev/null` (**the GH Actions shape**) | no | **FAIL** — 3 survivors |
| `sleep infinity` | no | **FAIL** — 3 survivors |

So the named suspects are eliminated by construction: the image's bash version,
its `procps` build, and running as root are held constant across a pass and a
fail. The failure also reproduces in the **real** `ci-base` image (built locally
from `ci/docker/ci-base/Dockerfile`, since ghcr needs auth) under
`--cpus=4 --entrypoint tail`, and does *not* reproduce there under `--init`.

`check-fast`'s 32-way parallelism is not involved either: with the fix, the test
passes 3/3 under a 32-way fork-storm on a 4-vCPU quota inside the CI image.

## Fix

Exclude zombies from "live members of a process group", at all three sites of
the idiom:

- `scripts/build/subtree-guard.sh` → `_nros_guard_group_members`
- `packages/testing/nros-tests/tests/subtree_guard.sh` → `group_members` /
  `group_size` / `members_still_in_group` (plus the two inline `ps -eo pgid=`
  predicates, now routed through `group_size`)
- `scripts/ci/runner-sweep.sh` → `_nros_sweep_group_members` and the main
  `_sweep_processes` scan

`ps -eo pid=,pgid=,stat=` + `$3 !~ /^Z/`.

**This is a production defect, not only a test one.** With the state-blind
predicate, on every CI container:

- `_nros_guard_cleanup` never sees its group drain, so it burns its full 10 s
  wait and then sends a pointless `SIGKILL` to a group of corpses — on *every*
  guarded build that is interrupted;
- `nros_guard_reap` counts corpses as members, so it either announces "reaping N
  orphan(s)" for nothing, or — if the recorded launcher pid is live — **refuses
  to start a build** over processes that already died;
- `runner-sweep`'s scan sees each zombie as reparented-and-unattributable (a
  zombie's `/proc/<pid>/exe` is unreadable) and tells the operator to re-run the
  sweep as another user, forever.

### Evidence the fix does not blind the test

Negative control: with `_nros_guard_cleanup` neutered to `return 0`, the **fixed**
test still fails in the GH-shaped container, and the survivors it reports are
genuinely alive:

```
FAIL: subtree survived SIGTERM to its launcher — 4 of ITS OWN process(es) still in pgid 822.
  PID  PPID  PGID STAT COMMAND
  822     1   822 Z    bash <defunct>     <- not counted
  825     1   822 S    bash               <- counted
  826   825   822 S    sleep              <- counted
  827   825   822 S    sleep              <- counted
```

## Reproducing it in one command

```sh
docker run -d --name gh-sim --entrypoint tail -v "$PWD":/w -w /w ubuntu:22.04 -f /dev/null
docker exec gh-sim sh -c 'apt-get update -qq && apt-get install -y -qq procps'
docker exec gh-sim bash /w/packages/testing/nros-tests/tests/subtree_guard.sh
```

The CI image is not needed to reproduce it, and neither is ghcr auth. What was
needed was matching the runner's **container invocation**, which is where the
issue's own "what is left" list put it second.

## Why it matters beyond the gate

The guard was working the whole time — but nothing in the tree could tell a
killed subtree from a live one under a non-reaping PID 1, which is the only
environment CI ever runs in. Had a genuine orphan appeared there, the guard's
reaper would have behaved the same way it did for corpses, so the failure mode
0762 exists to prevent was undetectable in CI either way.

## Not to do

Do not silence it to make `pr-checks` green. A gate reporting a reproducible
failure is doing its job; the value of W0.5 was finding that the reds were real
and unattended, not making them disappear.

## Follow-up worth considering

There is no gate for the class "a `ps`-based liveness predicate that does not
exclude `Z`". Three sites existed and all three were wrong; a fourth is one
copy-paste away, and the comment in `runner-sweep.sh` already advertises the
idiom as the one to copy.
