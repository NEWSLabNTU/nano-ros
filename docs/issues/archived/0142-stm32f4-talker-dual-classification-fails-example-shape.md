---
id: 142
title: "`examples/stm32f4/rust/talker` declares BOTH `[…nros.entry]` and `[…nros.node]` — `example_shape::component_or_application_classification_present` red since the 0100.W4 collapse"
status: resolved
type: tech-debt
area: examples
related: [phase-0100]
---

## Summary

`example_shape::component_or_application_classification_present` fails on
current main:

```
examples/stm32f4/rust/talker — declares MORE than one of component/application/entry
```

Commit `c6284c3a1` (2026-06-27, "fix(0100.W4): collapse stm32f4 talker
Entry/Node split into one crate") intentionally merged the Entry and Node
crates, leaving the single `Cargo.toml` with BOTH `[package.metadata.nros.entry]`
and `[package.metadata.nros.node]`. The shape test enforces exactly-one of
component(=node)/application/entry per leaf, so the collapse and the test
contract disagree — the test has been red since June 27 (masked locally
whenever `example_shape` wasn't in the run set).

## Fix direction (decide one)

1. If the collapsed single-crate shape is the intended 0100.W4 outcome, teach
   the classification test that `entry` + `node` may coexist on a collapsed
   leaf (or add an explicit `collapsed = true` marker it accepts), OR
2. drop the `[…nros.node]` table from the talker (the entry classification
   subsumes it) if nothing consumes it.

Owner of 0100.W4 should pick; either change is small.

## Resolution (2026-07-08) — already fixed; verified

The 0100.W4 collapse is intentional: the stm32f4 talker is a self-dispatching
Entry that IS its own node — it legitimately declares both
`[package.metadata.nros.entry]` (its deploy board) and
`[package.metadata.nros.node]` (the `TalkerNode` it registers). Option 1 was the
right call and has since landed: `example_shape::
component_or_application_classification_present` now mirrors the CLI schema
(`PackageMetadataNros::validate`) — `application` must stand alone, but `entry`
MAY coexist with a node/component; only the collapsed node+entry, node-alone,
application-alone, and entry-alone shapes are accepted. Verified:
`component_or_application_classification_present` PASSES (18 s). No code change
needed; closing.
