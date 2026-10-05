---
id: 1688
title: "A Rust workspace entry's launch `<remap>` does not reach the wire —
  `/remapped_out` never receives a sample"
status: open
type: bug
area: [codegen, testing]
severity: medium
found: 2026-10-05
related: [1651, 1684, 1687]
---

## Failing test

`nros-tests::workspace_features_e2e workspace_features::case_03_rust_remap`:

    [rust remap] /remapped_out never received 3 samples — the launch/model <remap>
    did not reach the wire (the publisher is on the wrong name)

## Evidence

- CI: workflow_dispatch run 37252649866, tree of commit
  dc5691a584cfb9f2a53b4b92372370d81ee8cd38.
- Local, SOLO (`-j1`), fixtures built from that tree 2026-10-05 02:43–03:24
  UTC by `just build-test-fixtures lane=tier1`: red again (22 s), so not load
  and not a stale fixture.

Possibly the same layer as issue 1687 (per-entity configuration from the model
not reaching the live entity) — filed separately because remaps and QoS take
different paths (`runtime.remaps` vs the QoS profile) and only a diagnosis can
join them. Not bisected; no lane ran this between 2026-06-17 and now (issue
1651).

## Acceptance

The cell passes solo and in the tier-1 `test-all`.
