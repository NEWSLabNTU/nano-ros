---
id: 861
title: "`[lifecycle] autostart = \"active\"` does not reach `active` at boot in
  the rust workspace-features cell"
status: open
type: bug
area: core, examples
related: [phase-263, phase-264, phase-331]
---

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
