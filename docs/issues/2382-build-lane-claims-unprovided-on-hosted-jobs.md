---
id: 2382
title: "The `build` gate lane claims submodules and full history that its hosted jobs never provide, so scheduled `gate` and `host-tests` run no build-tier gate"
status: open
type: bug
severity: high
area: [ci, gates]
related: [phase-543, phase-466, 1758]
found: 2026-11-09
---

**Home: [phase-543](../roadmap/phase-543-nextest-gates.md)** -- W5 added the
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

## What would close it

A scheduled `gate` whose build-tier shards and a scheduled `host-tests` whose
tier-1 gates run their gates (green, or red about the code).
