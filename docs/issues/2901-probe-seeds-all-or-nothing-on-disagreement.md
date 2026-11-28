---
id: 2901
title: "The metadata probe seeds launch values only when every instance agrees on every parameter; one disagreeing key drops the seeds for all of them"
status: open
type: feature
severity: medium
area: [cli, params, cmake]
related: [phase-478, issue-2640]
found: 2026-11-23
---

**Home:** [phase-478](../roadmap/phase-478-island-developer-experience-findings.md)
I52. Consumer: the safety island (autoware-safety-island fork), branch
`nano-ros-next`.

## What happens

Issue 2640 made the C/C++ metadata probe seed launch values, so a node may
declare parameters without defaults. The seeds are taken only when every
launch instance of the component agrees
(`packages/cli/nros-cli-core/src/orchestration/probe_seeds.rs`,
`Seeding::Disagree`). When the instances differ on ANY key, the probe
constructs the node on its declaration defaults for ALL parameters.

## Repro (island)

The controller is launched from two models: `system_model.yaml` and
`system_carla_model.yaml`. Their parameter files differ on `wheel_base`
(2.74 vs 3.005), `max_pitch_rad` and `min_pitch_rad`; the other parameters
agree. With the C++ `declare_parameter` defaults removed, the probe refuses:

```text
node cannot be constructed on its declaration defaults ... launch instances disagree
```

So the island cannot reach 2640's goal: it keeps all 148 declaration defaults
to cover three keys.

## Ask

One of:

- (a) fall back per parameter: seed every key the instances agree on, and
  leave only the disagreeing keys on their declaration defaults (the refusal
  then names only those keys when one has no default); or
- (b) probe per model: one probe per distinct seed set, the metadata merged
  (refused only if the recorded entities differ).

## Acceptance

- A fixture with two instances that differ on one key, that key with a
  declaration default and the rest without, is probed.
- The island removes its controller's declaration defaults except where the
  chosen answer still needs them.

## Reverted for nros12 (2026-11-28)

A fix (option (a): seed the agreed keys, default only the disagreeing ones)
landed in PR #5039 and was reverted before the nros12 release. With it, the
safety island's FreeRTOS POSIX parameter census read 148 declared against
150 seeded (`ego_nearest_dist_threshold`, `ego_nearest_yaw_threshold`
"seeded, but never declared"), where the build without it reads 150/150.

Not yet diagnosed. Two readings fit the numbers:

- stock Autoware's controllers write
  `has_parameter(p) ? get_parameter<double>(p) : declare_parameter<double>(p)`.
  Once the probe seeds a key, `has_parameter` is true during the probe, the
  declaration never runs, and the census misses the key;
- partial seeding takes the agreed LAUNCH keys without intersecting them with
  the keys the node declares, so a launch key no declaration names counts as
  seeded.

A later reading (2026-11-28) favours the first and says the fix was
right. Seeded, the code takes the `get` branch, so the code declares 148 keys
and the launch seeds 150. The image's entry seeds the same keys, and the
store sizing already counts seeds as declarations (issue 2768), so the store
is 150 slots either way. On that reading, the island's `check-param-seeds.py`
rule (seeded keys must be a subset of code-declared keys) is stricter than
the runtime. An intersect-with-declared fix would then be wrong: it drops
those keys' launch values from the probe. The redo would be #5039 as it was,
plus an update to the island's check. The store count has not been measured
on target.

If the parameter store is sized from that census, the image runs short of
slots.

**Re-landing needs:** the diagnosis above, then a fix where the census reads
the same with seeding as without it. Acceptance adds a fixture for the cause
found (a seeded key read through `has_parameter ? get : declare`, or a launch
key the node never declares), with the census asserted equal to the build
without seeding.
