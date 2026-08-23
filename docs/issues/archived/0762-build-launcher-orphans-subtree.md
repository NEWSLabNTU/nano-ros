---
id: 762
title: "Killing a build launcher orphans its whole subtree — make/just/cmake/cargo
  keep building against sources that have already moved"
status: resolved
type: tech-debt
area: build, tooling
related: [issue-0499, issue-0393]
---

## Problem

`just build-test-fixtures` is a shell script that runs `make`, which runs a
per-platform `just`, which runs another `make`, which runs `cmake`, which runs
`cargo`. Killing the top of that chain kills **one process**. Everything below
is reparented to init and keeps going.

Observed 2026-08-23. A `kill -TERM` of the launcher returned the prompt
immediately; ten minutes later this was still running:

```
77602   third-party/make/make -j20 --jobserver-style=fifo -f build/workspace-fixtures-make/ws-linux-...mk
141648  cmake --build build-workspace-fixtures-xrce --target native_entry
147726  cargo rustc --lib --target=aarch64-unknown-linux-gnu ... -p nros-cpp
```

Three things follow, and the third is the expensive one:

1. **The build continues against sources that have moved.** The reason for
   killing it was usually that the tree changed underneath it.
2. **A restarted build races the survivors** in the same target directories.
   Two cargo/cmake trees writing one directory is how artifacts get mixed.
3. **The damage is attributed elsewhere.** Here the interrupted build never
   wrote `target/nextest/.fixtures-built`, so `check-artifact-identity-budget`
   lost its `started_at` bound, fell back to counting every rlib in an
   accumulated tree, and failed with 12 `nros` identities against a ceiling of
   5 — a red that reads as a duplicate-compilation regression (issue 0499) and
   is nothing of the sort.

It is silent by construction: the terminal comes back, so the launcher looks
like it stopped.

## Cause

A signal sent to a PID reaches that process. Reaching a whole tree requires
signalling the process *group*, and no launcher in this repo established one or
trapped anything. There are ~10 fan-out sites (`build-test-fixtures`,
`workspace-fixtures-build.sh`, `jobserver-pool.sh`, the per-platform
`build-fixtures` recipes, …), so a trap per site would also mean the next site
added silently lacks one.

## Fixed 2026-08-23 — one guard, because a process group is inherited

`scripts/build/subtree-guard.sh`. `make`, `just`, `cmake` and `cargo` are all
already inside the launcher's process group, so **one** guard at the outermost
launcher covers the entire tree; per-site traps were never needed.

```sh
source scripts/build/subtree-guard.sh
nros_guard_exec fixtures make -j "$make_jobs" -f "$makefile"
```

### Nesting is the part that is easy to get wrong

If every level made its own process group, killing the top group would stop
reaching the levels below — the guard would defeat itself, and invisibly, which
is worse than not having it. So only the outermost caller creates a group;
`NROS_SUBTREE_GUARD` announces it to descendants and inner calls are a
transparent passthrough. That is what makes it safe to add at every launcher,
which is why it is added at every launcher.

### SIGKILL, and what makes the promise honest

A trap cannot cover `kill -9`. Nothing in a shell can. The second half is a lock
recording `<launcher-pid> <payload-pgid>`, so the NEXT build finds the survivors
instead of silently racing them:

* **launcher alive** → a build is genuinely running → **refuse**, and print the
  `kill -TERM -<pgid>` that stops it. Two fixture builds in one tree corrupt
  each other's artifacts, and "it was already broken" is how that gets
  misdiagnosed for hours.
* **launcher dead, group alive** → orphans → **reap**, announced. A build that
  silently kills processes it did not start is indistinguishable from one that
  hangs.

**The discriminator is the launcher, not the payload's group leader**, and
getting that backwards is the interesting bug — the first cut had it. After a
SIGKILL the payload leader is very much alive, so a payload-keyed check reports
"already running" and refuses *forever*, while the orphans it should have reaped
keep burning the machine. The test asserts this case by name.

### Coverage

`packages/testing/nros-tests/tests/subtree_guard.sh`, wired into `check-fast` as
`check-subtree-guard`. It drives real process trees four levels deep and asserts
on `ps` rather than on the guard's own log lines — a one-level payload would
pass against a guard that only killed its direct child. Three paths: trap,
refuse, reap. Each asserts the situation is real before asserting the remedy;
the reap case fails if the payload did *not* outlive its SIGKILLed launcher,
since that would mean the test had stopped exercising the orphan case at all.

Guarded launchers: `build-test-fixtures` (justfile), `workspace-fixtures-build.sh`
(the one that actually orphaned), `jobserver-pool.sh`, `native.just`'s example
fan-out.
