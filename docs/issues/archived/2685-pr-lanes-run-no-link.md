---
id: 2685
title: "The PR and merge-group lanes run no host C link: #4151 broke `check::build` on main for about 3 h"
status: resolved
type: enhancement
severity: medium
area: [ci, gates, build]
related: [phase-567, issue-2633]
found: 2026-11-17
resolved_in: "ci(phase-567 W7, issue 2685): an unconditional host C link smoke in CI (#4192)"
---

**Home: [phase-567](../../roadmap/archived/phase-567-regression-prevention.md)** (proposed
W7, a cheap merge-queue addition like W5). GitHub
[#4170](https://github.com/NEWSLabNTU/nano-ros/issues/4170). Unclaimed: the
nros 241 manager assigns the owner.

## What happened

#4151 (phase-560 W9.a, merge 48699e26c3, 2026-11-17 14:21Z) added a cffi
`Can` wrapper (`packages/platform/nros-platform-cffi/src/can.rs`) that calls
the five `nros_platform_can_*` symbols. Only a CAN backend defines them, so
`libnros_c.a` / `libnros_cpp.a` carried undefined references on every
non-CAN platform. The PR's `CI` and the merge queue's `CI` both passed: no
job on either event links a host C or C++ binary against the shipped archive.
`check::build` does, and it runs daily only (host-tests.yml since phase-559
W7; gate.yml's comment explains why it is not on a merge-gating event).

Every local `just ci gate` then failed in `check::build`: `check::c`,
`check::cpp`, `staticlib-symbols` and `borrowed-e2e`, each with

    can.rs:73: undefined reference to `nros_platform_can_open'

#4158 (61364bbd78, 17:15Z) put the wrapper behind an off-by-default `can`
feature. For the 2.9 h between, every agent's full gate was red for a cause
outside its diff, including the nros12 blocker #4132.

## Proposal

A link smoke heard by the required check `CI`: the job `ci-ok` in
`.github/workflows/gate.yml`, the one status context the ruleset requires.

**Unconditional.** No `paths:` filter, and no skip on `changes.outputs.code`
(the docs-only output that excuses the `check-*` siblings on a pull request).
A required check that gives no verdict blocks forever; that deadlocked two
PRs once. The smoke runs on `pull_request`, `merge_group` and `push`, and
`ci-ok` treats a skip of it as a failure on every event.

**What to run: a minimal `check c` link.** Factor the first half of
`check::c` (`just/check/lanes.just`) into its own recipe, for example
a new `check::c-link-smoke` (not yet written), and have `check::c` depend on it so the build and
link keep one spelling:

1. `nros_cargo_placed ... build -p nros-c --no-default-features --features
   "$C_API_SHIPPED_FEATURES"` into its own scoped target dir;
2. `cc` link `packages/api/nros-c/tests/run/cdr_borrow_le_slice.c` against
   that `libnros_c.a` (`-lpthread -ldl -lm`), then run it.

**Where it runs.** Two shapes; the owner picks:

- a sibling job (`check-link-smoke`) with no `if:` and no `needs: changes`,
  listed in `ci-ok`'s `needs` and in its sibling table for every event, so
  `check-default-gates-run-somewhere` and `check-merge-group-gates` see it;
- a step in `ci-ok` itself. `ci-ok` today has no checkout, no container and a
  10 min timeout, so this adds the image pull and the CLI build to the
  aggregator's own critical path.

The sibling keeps `ci-ok` a pure aggregator; the step is literally inside the
required job. Both are unconditional.

**Cache in CI.** The link needs the nros CLI (`nros locate artifact` for the
generated header dir) and a workspace that resolves, so the job uses the
`check-fast-compile` setup: `./.github/actions/setup-nros-cli` (binary cache
keyed on the CLI source stamp) and the compile-tier source provisioning. The
Rust compile goes through the existing sccache restore (`sccache-Linux-check-`
prefix, `/tmp/sccache`): the nros-c closure is the same crates
`check-fast-compile` and `check-compile-smoke` compile, so a warm cache turns
the cold 52 s of CPU below into mostly cache hits. No new cache key is
needed. On a `push` that the queue already verified the smoke still runs; at
its warm cost that is cheaper than a new skip rule.

## Measured

This host: 20 cores visible, load average 23 to 44 during the runs, every
run under `nice -n 10`, a fresh worktree on 61364bbd78 (`just setup-cli`
done, no shared cargo target). "Reverted" is the same tree with #4158's three
package files reverse-applied (`git show 61364bbd78 -- packages | git apply
-R`), so it is the #4151 state.

| candidate | cold | warm | reverted (incremental) | catches #4151 |
| --- | --- | --- | --- | --- |
| `just check staticlib-symbols` | 35:43 wall, 396 s user | 14.6 s | (fixture below fails first) | yes |
| its fixture alone, `scripts/build/link-determinism-fixture.sh` | part of the above | 0.75 s | 15.0 s, rc 1 | yes |
| minimal `check c` link (above) | 26.8 s wall, 52 s user | 0.32 s | 40.5 s wall, 3.9 s user, rc 1 | yes |

The `staticlib-symbols` cold wall time is mostly waiting: its two compiles
took 1 min 05 s (the zenoh image crate, zenoh-pico's C included) and 1 min
00 s (`nros-tests` for the one nextest test); the rest was cargo's package
cache lock, held by other agents' builds on the shared `~/.cargo`. Its CPU,
396 s user, is still 7.6 times the minimal link's 52 s. The reverted runs
failed with the incident's own lines:

    can.rs:73: undefined reference to `nros_platform_can_open'
    can.rs:105: undefined reference to `nros_platform_can_recv'
    can.rs:80: undefined reference to `nros_platform_can_send'

## Recommendation

The minimal `check c` link. It catches #4151, costs about half a
minute cold here with no C dependency build and no test-crate compile,
and is under a second warm. `staticlib-symbols` also catches it but compiles
zenoh-pico and `nros-tests` cold. Its fixture alone is as cheap warm but
still builds zenoh-pico cold (about 1 min here), and it links a different shape (zenoh backend bundled, `-u` force), which is the
determinism question and not the one this issue asks.

Not covered: C++ (`libnros_cpp.a`) links, and archives built for a platform
other than POSIX. #4151 broke the C++ link too, and the C link caught the
same undefined symbols, so one link is enough for this class.

## Acceptance

- Reverting #4158's package hunks fails `CI` on a pull request and in the
  merge queue, at the smoke, naming `nros_platform_can_open`.
- A docs-only pull request still runs the smoke, and `CI` reports a verdict.
- `check::c` calls the smoke recipe; the build and link are spelled once.
- The CI cost (cold and warm, hosted runner) is recorded in phase-567.
