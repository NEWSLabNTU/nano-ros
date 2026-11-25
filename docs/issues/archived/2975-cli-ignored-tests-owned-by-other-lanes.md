---
id: 2975
title: "`test-ignored` ran the cli sub-workspace's colcon tests on a runner with no colcon, so tier-2 nightly failed by construction"
status: resolved
resolved: 2026-11-26
resolved_in: "the change that adds this issue"
type: bug
severity: high
area: [testing, ci]
related: [issue-1507, phase-526]
found: 2026-11-26
---

Release blocker B42. Found in the nros12 release dispatch at `01388e07a3`.

## What happened

Tier-2 nightly (run 45684820840, job 139498579863, the 118 runner) failed in
its `test-ignored` step on three `nros-cli::colcon_flag_parity` tests:

```text
colcon is not on PATH (Err(Os { code: 2, kind: NotFound, ... })); install
`colcon-common-extensions` (`nros setup` provisions it) or run without --ignored
```

## Cause: scope, not the image

The runner image has no colcon. That is by design: `[lane.runner]` in
`.config/gate-lanes.toml` claims no colcon ("no bindgen or colcon"). Like
`west`, an index entry with a command is installed by a provisioning verb, not
baked into the image (`scripts/ci/lib/image-context.sh`).

The three tests are `#[ignore]`d because they need colcon and the
`ros2_rust_examples` submodule. Their owner is `just check cli-tests`, which
runs them only where both are present (phase-526 W3).

`test-ignored` runs every `#[ignore]`d test (issue 1507), minus the ones
another lane owns. For the ROOT workspace that subtraction is
`[profile.ignored]` in `.config/nextest.toml`. The `packages/cli`
sub-workspace reads no such profile, so nothing dropped the cli-owned tests,
and they failed wherever colcon is absent.

Not a regression. None of the five earlier nightlies on this runner reached the
step, because each stopped at an earlier failure. 01388e07a3 is the first
nightly that got this far.

## Fix

The sub-workspace run in `test-ignored` now applies the same rule inline:
`-E "not (binary(colcon_flag_parity))"`, with its owner named beside it as the
root profile does. `nextest list` of the sub-workspace's ignored set goes from
26 tests to 23, and none of the 23 is `colcon_flag_parity`.

A new ignored cli test that another lane owns goes on that list. One with no
owner stays in `test-ignored`, which is what keeps it from rotting unseen.
