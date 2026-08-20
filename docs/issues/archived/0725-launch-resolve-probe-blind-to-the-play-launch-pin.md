---
id: 725
title: "`setup-launch-resolve`'s staleness probe compares layer-2 CONTENT while the 0409 guard compares the play_launch COMMIT, so a pin move that touches only `tests/` deadlocks every build"
status: resolved
type: bug
area: cli, build
related: [issue-0561, issue-0409, issue-0596, issue-0419, rfc-0060]
---

## Symptom

`just build-test-fixtures` cannot run, and the remedy it prints cannot clear it:

```
Error: sync: `…/nros-launch-resolve/target/release/nros-launch-resolve` was built from
play_launch 420904826055 but this `nros` was built from 65a7591e5165.
    ./scripts/bootstrap.sh      (contributors: just setup-launch-resolve)
(issue 0409)
error: recipe `generate-bindings` failed
```

Run the suggested command and it exits **0**:

```
$ just setup-launch-resolve ; echo rc=$?
rc=0
$ …/nros-launch-resolve --version
nros-launch-resolve 0.5.0 (play_launch 420904826055…)   # unchanged
```

Repeat forever. There is no sanctioned command that fixes it.

## Cause — two identities for one question

`build.rs` bakes the play_launch submodule commit into BOTH binaries as
`NROS_PLAY_LAUNCH_SHA`, and `verify_resolver_pin` (`ws.rs`) compares those two
**commits**.

`nros_launch_resolve_stale` decides whether to rebuild by hashing layer-2
**content** — every `.rs` / `Cargo.toml` / `Cargo.lock` under
`packages/cli/nros-launch-resolve` and `play_launch/src/ros-launch-resolve`.

Those two identities agree only while every pin move touches
`src/ros-launch-resolve`. `420904826055..65a7591e5165` does not:

```
$ git -C packages/cli/third-party/play_launch diff --stat 4209048 65a7591 -- src/ros-launch-resolve
                                    # empty
$ git -C … diff --stat 4209048 65a7591 | tail -1
 9 files changed, 76 insertions(+), 473 deletions(-)   # all under tests/
```

So the content stamp matched, the probe said "current", the recipe exited 0
without rebuilding, the binary kept the old commit, and the guard kept refusing.

## This is issue 0561, one binary over

0561 is the same defect in `nros` itself, and `source_stamp.rs` still carries
its account verbatim:

> Issue 0561: this is a CLI build input even though it is not a file under any
> watched directory. `build.rs` bakes it as `NROS_PLAY_LAUNCH_SHA` and the
> issue-0409 guard compares that value, so a stamp blind to it disagreed with
> the build in the one case that mattered — moving the pin left the stamp
> unchanged, `setup-cli` skipped the rebuild while reporting success, and no
> sanctioned command could clear the resulting mismatch.

Two binaries stamp `NROS_PLAY_LAUNCH_SHA`. 0561 fixed one. The other kept the
bug for as long as it took a pin to move without touching layer 2 — which is
CLAUDE.md's "fix the CLASS, not the reported site" with the sibling named in the
fix's own comment.

## Fix

`nros_launch_resolve_stamp` now folds the pin into the digest, mirroring
`play_launch_pin` in `source_stamp.rs` — including its issue-0419 gate on the
`.git` FILE, because `git -C <empty dir> rev-parse HEAD` walks UP to the
superproject and would re-stale the resolver on every nano-ros commit.

The stamp is now a strict superset of what the guard compares, so the two
cannot disagree: any pin move makes the stamp differ, the rebuild runs, and the
commits match again.

Verified:

* probe flips to STALE at the mismatching pin (it reported "current" before);
* `just setup-launch-resolve` then rebuilds, and `--version` reports
  `play_launch 65a7591e5165…`, equal to `git ls-tree HEAD` on the submodule;
* the stamp is still invariant to how the caller spells the root — identical
  digest for `.` and for the absolute path, which is issue 0596's property and
  the reason the digest holds repo-relative paths.

## Why no gate

The invariant is "the rebuild probe watches at least what the runtime guard
compares", and it has exactly two instances, now both correct and now both
computing the pin the same way. A gate over two call sites costs more than it
catches; what protects the next one is that `source_stamp.rs` and
`launch-resolve-stale.sh` each name the other.
