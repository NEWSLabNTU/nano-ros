---
id: 1030
title: "check-lane-contracts covers merge-gating lanes only, so a schedule-only step can consume an artifact no step on that event builds — the scheduled gate was red four days"
status: resolved
area: ci, testing
severity: medium
found: 2026-09-04
resolved: 2026-09-04
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

## Fixed

`check-lane-contracts` now carries the rule, rather than a second gate beside
it — the event vocabulary (`_events_of`) and the workflow scan already lived
there.

**A second declaration source.** `ordered_setup_requirements` reads the
justfile's own dependency ORDER:

    _codegen: setup-launch-resolve generate-bindings _require-leaf-includes

`just` runs dependencies in order, so this states that `generate-bindings`
needs `setup-launch-resolve` first. That is a declaration someone wrote for
their own reasons, which makes it a cross-check rather than a table this gate
maintains and lets rot. It complements the existing `required_producers`, which
reads a recipe's hard-failing remediation text; neither subsumes the other —
`generate-bindings` fails inside `nros sync`, a Rust binary whose message no
recipe body contains, so only the ordering sees it.

**Event-level, not presence.** The tier rules ask "does the job run the
producer", and gate.yml's `check` job does — on pull_request/merge_group. The
new rule compares EVENT SETS, which is the only way this defect is visible.

**Not restricted to `GATING_EVENTS`.** That restriction is a stated severity
call for the tier rules and stands. It does not apply to a step that cannot run
at all on its own event.

## The scanner bug underneath

The rule could not fire when first written, and the reason was worth the trip:
`workflow_jobs` read only the `if:` LINE, so a folded guard —

    if: >-
      ${{ contains(fromJSON('["pull_request","merge_group"]'), github.event_name)

— contributed no event names and `_events_of` returned "every event the
workflow declares". The producer was therefore credited with every event, so it
covered every consumer and no gap could exist. For the tier rules that
over-inclusion is the direction their doc calls safe; for this rule it is the
unsafe one, and it blinded the gate to precisely the step whose guard was too
narrow. The scanner now appends the guard's continuation lines.

## Verified

* Against the UNFIXED `gate.yml`, one finding and no false positives:
  `gate.yml:check runs `just generate-bindings` on schedule,workflow_dispatch
  without `just setup-launch-resolve``.
* Against the FIXED `gate.yml`, green. So the proof is the real defect and its
  real fix, not a synthetic mutation.
* Four new self-tests (26 total, 0 failed); `just check fast` 190/190.

Two earlier predicates were measured and REJECTED rather than baselined:
"a step whose events are a strict subset of a later step's" flagged 17+ pairs
with no dependency between them (`sccache stats` "consuming" `cli-tests`), and
narrowing to `setup-*` producers still gave 8, of which 6 were false. A gate
with a six-entry false baseline is the issue-0309 silent-lane class in advance.

## What this does NOT cover

Only requirements the justfile DECLARES through dependency order. `just check
cli-tests` needs the same binary and is not declared that way — its requirement
surfaces as a runtime panic in `plan_pipeline_e2e.rs` — so this rule does not
see it, and the summary line says how little it covers (1 invocation today)
rather than printing a bare OK. Declaring more requirements widens the rule for
free; inventing a wrapper recipe nobody calls, purely to feed the gate, would
not, so it was not done.
