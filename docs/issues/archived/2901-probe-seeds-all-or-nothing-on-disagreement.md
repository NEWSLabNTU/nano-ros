---
id: 2901
title: "The metadata probe seeds launch values only when every instance agrees on every parameter; one disagreeing key drops the seeds for all of them"
status: resolved
type: feature
severity: medium
area: [cli, params, cmake]
related: [phase-478, issue-2640]
found: 2026-11-23
resolved: 2026-11-25
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

## Resolution

Fixed 2026-11-25 with answer (a), the user's ruling. When instances
disagree, `Seeding::Disagree` carries the keys they agree on: every key each
instance sets, to one value. The probe is seeded with those. Only the
disagreeing keys stay on their declaration defaults, and a key absent from one
instance still counts as a disagreement. The refusal for a probe that still
fails names only the disagreeing keys and the remedy: give each a default
where the node declares it, or make the instances agree. The sidecar records
`probe.partly_seeded` (instances, seeded key names, disagreeing keys, sources,
digest). Sidecars that hold the old `unseeded` record still parse.

Tests:

- `a_partial_disagreement_seeds_every_agreed_key_and_names_only_the_others`
  covers the island's shape, three disagreeing keys among agreed ones.
- `fixtures/probe_seeds_ws` has a second C++ instance, `seeds_cpp_alt`. It
  agrees on `ctrl_period_ms`, which has no default, and sets `gain_03`, which
  has one. Probed on host 241, the C++ sidecar records its timer at the launch
  period, all 41 parameters, and `partly_seeded` naming `gain_03`.
  `tests/probe_seeds_workspace.rs` asserts that.
