---
id: 1030
title: "check-lane-contracts covers merge-gating lanes only, so a schedule-only step can consume an artifact no step on that event builds — the scheduled gate was red four days"
status: open
area: ci, testing
severity: medium
found: 2026-09-04
related: [0876, 0878, 0196, 0993]
---

# A producer step narrower than its consumers, on a lane no gate reads

## What happened

`gate.yml`'s scheduled run failed on 2026-09-01, -02, -03 and -04 — every run
since the step was added — with two failing steps that had one cause:

```
Error: sync: 1 SystemModel(s) need resolving but `nros-launch-resolve` is not next to the `nros` binary:
  demo_bringup — .../examples/templates/c-and-cpp-mixed-workspace/build/nros/models/demo_bringup/system_model.yaml is missing (system.launch.xml)
Build it:  ./scripts/bootstrap.sh   (contributors: just setup-launch-resolve)
```

and, in `just check build`'s one failing gate of 21:

```
thread 'fixture_workspace_plans_and_checks' panicked at nros-cli-core/tests/plan_pipeline_e2e.rs:228:5:
nros-launch-resolve not built at .../packages/cli/nros-launch-resolve/target/release/nros-launch-resolve — run `just setup-launch-resolve`
```

`Build nros-launch-resolve` was conditioned on `pull_request`/`merge_group`,
because it was added for `just check cli-tests`. Two later steps need the same
binary on `schedule`/`workflow_dispatch` — `just generate-bindings` (via `nros
sync`) and `just check build` — so on those events the producer was SKIPPED and
both consumers failed.

Fixed by widening that step's event set to the union of its consumers'.

## Why the gate did not catch it

`check-lane-contracts` exists for exactly this shape — "a gate in an
affordability tier may only resolve artifacts the JOB ITSELF builds" — and it
passes here, reporting `7 merge-gating CI lane invocation(s)`. Two reasons it
cannot see this one:

1. **Scope.** `GATING_EVENTS = {"pull_request", "merge_group"}`; a lane whose
   events miss that set is skipped by construction
   (`scripts/check-lane-contracts.py:589`). That is a deliberate,
   documented severity call — a broken tier on `schedule` is a bad nightly, a
   broken tier on `merge_group` is a repository nobody can merge into — and
   this issue does not propose reversing it.

2. **Predicate.** Even in scope, the gate asks whether a RECIPE's closure
   resolves a stamp its job does not build. This defect is one level up: a
   workflow STEP that builds a binary, whose `if:` event set is a strict subset
   of the event sets of the steps that consume it. Nothing checks that
   containment, on any lane.

## Why it matters beyond the four days

A lane red for four consecutive runs has no signal capacity — the same property
issue 0876 rode in on. A regression landing in the scheduled gate would have
looked exactly like 2026-09-01's failure, and `check build` + `no_std` +
`generate-bindings` are the only place several gates run at all.

## What a fix looks like

A check that, per workflow job, every step producing a binary has an event set
that is a SUPERSET of every step consuming it. The hard part is deriving
consumption: `just check cli-tests` needs the resolver through a test that
panics at runtime with a path, which is not statically visible from the recipe.
An authored producer→consumer map would work and would also drift — the
`rmw-api-parity` lesson — so it needs cross-checking against something
observable, the way `check_against_vtable` does.

Cheaper interim: assert that no step in a workflow job is conditioned on a
STRICT SUBSET of the events of a step that appears after it and runs `just`,
unless listed in a baseline. Noisy, but the baseline makes new ones loud, which
is the property that was missing.
