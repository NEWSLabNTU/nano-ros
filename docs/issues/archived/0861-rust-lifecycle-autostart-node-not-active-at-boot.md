---
id: 861
title: "`[lifecycle] autostart = \"active\"` does not reach `active` at boot in
  the rust workspace-features cell"
status: resolved
resolution: retracted
type: bug
area: core, examples
related: [phase-263, phase-264, phase-331]
---

> **RETRACTED — not a defect.** The original report is kept below
> unaltered, because a retraction that deletes its own evidence cannot
> be checked.

## Retracted 2026-08-28 — the run that produced this was invalid

This was filed from a `just ci-matrix` run whose FIXTURES PREDATED THE TREE.
The sweep built its artifacts at 03:19, the tree was then rebased past 04:48,
and the old results were read as current. Re-run against fixtures matching the
tree, it passes.

That is not a flake and not a partial fix: nothing about the reported behaviour
was real. See the retraction note at the bottom for the shared cause, which took
all four issues from that run.

## Re-check

```
$ cargo nextest run -E 'test(workspace_features::case_01_rust_lifecycle)'
    PASS [   2.108s] (1/1) nros-tests::workspace_features_e2e workspace_features::case_01_rust_lifecycle
     Summary [   2.121s] 1 test run: 1 passed
```

Passes in 2.1 s against freshly built native fixtures, having failed at ~30 s
against stale ones. The autostart path works: `[lifecycle] autostart = "active"`
does reach `active` at boot.

## What I got wrong in the original analysis

The report made much of the observed state printing EMPTY after `got:`, and
proposed separating "no services registered" from "services present, state
wrong", with a side-suspicion of the queryable-slot budget. The empty string was
simply a node that never came up from a stale image — there was no state to
read. Every branch of that investigation plan led nowhere.

---

# Original report (retracted, kept for the record)
## Symptom

`nros-tests::workspace_features_e2e workspace_features::case_01_rust_lifecycle`
fails after ~30 s:

```
[rust lifecycle] expected the autostart-managed node /talker to be `active` at
boot (phase-263 A3: `[lifecycle] autostart = "active"` + nros/lifecycle-services
— nros::main! (phase-264 W2) registers the 5 REP-2002 services and drives
Configure→Activate at boot), got:
```

The observed state prints EMPTY after `got:` — the node's lifecycle state was
not merely wrong, it could not be read. That is a different failure from
"stuck in `unconfigured`", and the distinction should be settled before
anything is concluded: an empty read is consistent with the services never
being registered, with the node never coming up, and with the query itself
failing.

## Contract under test

Two things must both hold, and the test cannot currently tell you which broke:

* `nros::main!` registers the five REP-2002 lifecycle services (phase-264 W2),
* and drives Configure -> Activate at boot when `[lifecycle] autostart =
  "active"` is authored (phase-263 A3).

## Next measurement

1. Does `/talker` appear in the graph at all? If not, this is not a lifecycle
   bug.
2. Are the five lifecycle services registered? `ros2 service list` against the
   fixture separates "no services" from "services present, state wrong".
3. Only then, whether Configure -> Activate ran and what it returned.

Worth checking the service-count budget while there: a service server IS a
zenoh queryable, and `[lifecycle]` claims five slots before the app declares
anything (see the queryable-cap note in CLAUDE.md / issue 0460). A silent
registration failure from an exhausted table would look exactly like this.

## Repro

    source ./activate.sh
    just build-test-fixtures lane=tier2
    cargo nextest run -E 'test(workspace_features::case_01_rust_lifecycle)'

## Provenance

Found by the first full tier-2 run in some time (2026-08-28); pre-existing on
main and unrelated to the work landing alongside it.
