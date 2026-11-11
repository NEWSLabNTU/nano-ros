---
id: 2382
title: "The `build` gate lane claims submodules and full history that its hosted jobs never provide, so scheduled `gate` and `host-tests` run no build-tier gate"
status: resolved
resolved: 2026-11-11
resolved_in: "#3444 (disk reclaim, NOT VERIFIED self-test) + #3457 (provision-gate-lane.sh, gate-lane-provisioned)"
type: bug
severity: high
area: [ci, gates]
related: [phase-543, phase-466, 1758]
found: 2026-11-09
---

**Home: [phase-543](../../roadmap/phase-543-nextest-gates.md)** -- W5 added the
lane claims and their preflight, and W6 put `check build` through the harness.

## Measured

Since 2026-11-07 (phase-543 W5/W6: `f2c99b3330`, `40ec2d36af`, `2bc794fe21`),
every job that runs the `build` lane on a hosted runner stops before any gate:

- scheduled `gate`: all three `check / build tier + no-std [n/3]` shards, on
  11-06 (a different red), 11-07, 11-08 and 11-09 (run 42548650793);
- scheduled `host-tests`: `tier-1 gates (just check) [1/2]` and `[2/2]`
  (run 42555181729).

The last green scheduled `gate` was 2026-11-05 (`35494e2a4a`). Each job prints:

```
gate-lane-preflight: lane `build` claims what this host does not have:
  submodules: submodule  third-party/nuttx/libc; submodule  third-party/freertos/kernel
    (or: nros setup --source freertos-kernel); ... third-party/freertos/lwip;
    third-party/threadx/kernel; third-party/threadx/netxduo;
    packages/cli/testing_workspaces/ros2_rust_examples
      fix: just setup-worktree
  git-history: the clone is shallow
      fix: git fetch --unshallow
  ... No gate ran.
error: recipe `build` failed with exit code 1
```

## Why

`.config/gate-lanes.toml` `[lane.build]` describes the hosted jobs (its
comment names `check / build tier` and host-tests' tier-1 gates) and claims
`submodules` and `git-history`. Those jobs check out a shallow superproject and
initialise only the submodules whose pin moved
(`Init submodules whose pin moved (commits only)`), so neither claim holds. The
preflight is right that the claims are false; the claims and the jobs were
written apart, and nothing compares them.

The two ways out are about the lane, not about any gate:

- the jobs provide what the lane claims (`fetch-depth: 0` and
  `just setup-worktree` or the store-first `nros setup --source ...` for the
  named rows), at their measured cost; or
- the lane claims only what the jobs provide, and the gates that need the
  rest move out of its scope (named, never passed, the phase-543 rule).

Either way, `check-lane-contracts`' shape applies one level up: a lane's claims
should be checked against what its jobs provision, so a claim the job never
satisfies is caught by a gate, not by three nights of no verdict.

## Fix

The jobs now provide what the lane claims. The claims did not narrow:
`build` includes the fast tier, whose citation gates need history, and its
build tier reads the `# check::build` rows.

- `scripts/ci/provision-gate-lane.sh <lane>` reads the lane's `tiers` and
  `claims` from `.config/gate-lanes.toml` and fetches what the job must:
  - `submodules`: the `.config/worktree-provisioning.txt` rows tagged with the
    lane's tiers, at depth 1;
  - `git-history`: the base branch's commits, treeless;
  - `zephyr-tree`: the sparse header clone;
  - `sources`: rosidl.

  It replaces `provision-fast-lane.sh` and the fast jobs' two inline copies of
  the history fetch.
- gate.yml's `check-build` shards and host-tests' `gates` shards call it with
  `build`. The two fast jobs, `check-fast` and `warm-cache`, call it with
  `fast`.
- `just check gate-lane-provisioned` (fast, buildless) parses every workflow:
  a job that sets `NROS_GATE_SCOPE` to a lane claiming a fetched capability
  must run the script for that lane in an earlier step. Its self-test covers
  four refusals: a missing step, a step after the lane, the wrong lane, and an
  unknown lane. Against main's workflows it names exactly the four jobs above.

Measured on a fresh `git clone --depth 1` of the repository: before, the
preflight refused `submodules` and `git-history`. `provision-gate-lane.sh build`
took 49 s (15,883 commits, plus the depth-1 submodules). Afterwards the
preflight named only `cli`, which the test clone did not build and which every
CI job builds in an earlier step.

## What would close it

A scheduled `gate` whose build-tier shards and a scheduled `host-tests` whose
tier-1 gates run their gates (green, or red about the code).

## Sighting 2026-11-10: past the preflight, refused by the free-disk floor

Scheduled `gate` run 42739859713 (main at 78baf6859c) got past the lane
preflight this time, then all three `check / build tier + no-std [n/3]` shards
stopped at the next claim the hosted jobs cannot meet, before any gate ran
(jobs 129578033991, 129578034025; the same line inside job 129578033970):

```
nextest-gates: 29 GiB free under /__w/nano-ros/nano-ros, below 30 GiB
error: recipe `nextest-gates` failed with exit code 1
```

The floor is phase-543 W4's `"${NROS_GATE_MIN_FREE_GIB:-30}"` in
`just/check/gates.just` (9e6e3253e5). All three jobs ran on GitHub-hosted
runners ("GitHub Actions 1000140647/48/50"), whose free space is fixed by the
image, so `trim-caches.py` cannot meet it. This is the same class as this
issue: the `build` lane claims a host property its hosted jobs never provide.
Closing it needs one of three things: a measured `NROS_GATE_MIN_FREE_GIB` for
the hosted shards, space freed on the image and asserted before the floor, or
the shards moved to the self-hosted fleet.

Shard 3 reports it as `pristine-checkout SELFTEST FAILED: a gate that reads a
file inside an uninitialised submodule passed in the clone (rc=1)`. That
reading is wrong: rc=1 came from the floor refusing before any gate ran. The
self-test should read a preflight or floor refusal as NOT VERIFIED, not as a
gate that passed. (Issue id 2408 was reserved for this sighting and left
unused once it proved to be this class.)

### Fix for the floor (2026-11-10)

- **Free the disk, measured.** `check-build`'s container now mounts the
  runner's preinstalled toolchains (Android SDK, .NET, GHC/ghcup, Swift,
  PowerShell), the same list `host-tests.yml`'s gate shards mount. Its existing
  `reclaim-disk.sh` step had no mounts, so it reclaimed only the tool cache.
  On the gate shards of scheduled host-tests run 42747790725, the same reclaim
  freed 32,744 MB and left 51 GiB free, in 107 s and 153 s. That clears the
  30 GiB floor from 29 GiB with room to spare.
- **The floor stays 30 GiB for now.** The backstop, a hosted
  `NROS_GATE_MIN_FREE_GIB` set from the peak the build lane actually uses, needs
  that peak. These shards have never reached their gates, so no run has
  recorded it. The job's `disk-report.sh` after `just check build` records it,
  and the floor is set from the first scheduled run that gets there, not from
  a guess.
- **The self-test reads a refusal as NOT VERIFIED.**
  `scripts/ci/pristine-checkout-fast.sh` now tells the cases apart. A runner
  that exits non-zero with no record of the planted probe never ran it. That
  case prints `pristine-checkout SELFTEST NOT VERIFIED`, with the runner's own
  refusal lines, and stays red; it no longer claims the probe passed. Measured
  locally with `NROS_GATE_MIN_FREE_GIB=100000`: the new message, quoting the
  floor's `below 100000 GiB` line. The normal path is unchanged (513/513 in the
  clone, and the control still fails as it should).

## Acceptance (2026-11-10, dispatched on main `62f543fe33`)

Both fixes merged after the last scheduled runs (#3444 at 06:52Z, #3457 at
09:32Z; the scheduled `gate` ran at 02:01Z and `host-tests` at 03:11Z), so
they were dispatched by hand:

- `gate` run 42848962439: all three `check / build tier + no-std [n/3]`
  shards **green** (jobs 129944198902, 129944199587, 129944198984). These are
  the first green build-tier shards since 2026-11-05.
- `host-tests` run 42849000957: `tier-1 gates [2/2]` **green**.
  `tier-1 gates [1/2]` ran its gates and was then cancelled by the job's
  120-minute cap. Its gate log (artifact `host-tier1-gates-ci-logs-1`)
  shows the fast tier at 545/545 passed and the build tier at 32 of 47 with
  no failure when the cap hit.

So the lane's claims hold, and every job reaches its gates, which is what this
issue required. The cancelled shard is a time budget, filed as
[issue 2141](2141-host-tests-tier1-gates-outgrow-two-hours.md). Its time went
to `just generate-bindings` (16:01:48 to 16:45:07, 43 min), the fast tier
(565 s), and the build tier's slowest gates (`cpp` 756 s,
`ament-census-sizing` 632 s, `required-features-tests` 624 s, `borrowed-e2e`
595 s, `source-gates` 550 s) on a 4-CPU hosted runner.
