---
id: 2901
title: "The metadata probe seeds launch values only when every instance agrees on every parameter; one disagreeing key drops the seeds for all of them"
status: resolved
resolved: 2026-11-28
resolved_in: "the change that adds this line"
type: feature
severity: medium
area: [cli, params, cmake]
related: [phase-478, issue-2640]
found: 2026-11-23
---

**Home:** [phase-478](../../roadmap/phase-478-island-developer-experience-findings.md)
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

## Fixed (2026-11-28): option (a), seed per key

When a component's launch instances disagree, the probe now seeds every key
they agree on (`Seeding::Disagree { agreed, .. }`, which `params()` returns),
and only the keys they disagree on run on their declaration defaults. A key
one instance omits is not agreed, since there it takes its default. The note
and the refusal name only the disagreeing keys, and a refusal now tells the
user to give each of those keys a declaration default or make the instances
agree on it. The old "probe one instance per bringup" remedy is gone, because
option (b) was not taken.

The sidecar's `probe` object records the split as
`partial_seeds { instances, seeded, defaulted, sources, digest }`.
`unseeded` stays for instances that agree on no key. The probe key's seed
digest is unchanged: it still covers every instance's values, so editing a
disagreeing value still re-probes.

**Acceptance item 1.** `fixtures/probe_seeds_ws` now launches the C++
component twice. Both instances set `ctrl_period_ms`, which has no default,
to 25, and they set `gain_03`, which has default 3, to 7 and 9.
`tests/probe_seeds_workspace.rs` asserts that the sidecar records the timer
at 25 ms, all 41 parameters, and `partial_seeds` with `seeded =
[ctrl_period_ms]` and `defaulted = [gain_03]`.

Negative control: with main's `probe_seeds.rs` and the same fixture, that
test fails with `left: []`. No timer is recorded, because the one
disagreeing key dropped the period's seed too.

**Acceptance item 2** is the island's step: removing its controller's
declaration defaults except for `wheel_base`, `max_pitch_rad` and
`min_pitch_rad`, the three keys its two models disagree on. Phase-478 I52
tracks it.
