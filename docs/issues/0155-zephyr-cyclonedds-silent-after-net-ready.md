---
id: 155
title: "Zephyr+CycloneDDS lane: images boot, net ready, then silence — no publish, no error (all 8 phase_118 tests)"
status: open
type: bug
area: zephyr
related: [issue-0152, phase-282]
---

## Summary

All 8 `phase_118_collapse` zephyr-cyclonedds tests fail on freshly rebuilt
images (build-{rs,c,cpp}-*-cyclonedds wiped + `NROS_ZEPHYR_FIXTURE_FILTER=
cyclonedds just zephyr build-fixtures`): the image boots, prints
`nros_net_wait: Network ready (NSOS)` — then NOTHING. No publish log, no
session-open error, no crash. Rust boot tests fail in 3-4 s (no output in
window); pubsub/service e2e in ~50 s. The C lane also shows a
`cyclone: Failed to start RTPS` info line in earlier runs.

Not triaged to a cause yet. Candidates, in test order:

1. **Stash-baseline first** (env_machine_test_debt rule): does the lane pass
   with the phase-282 tx_express / QoS-struct commits stashed and images
   rebuilt from the pre-diff tree? `NrosRmwQos.tx_express` was carved from
   `_reserved0` (layout-stable by design) and the cyclone cffi ignores it,
   but the embedded-cyclone lane is the one consumer that was NOT
   re-validated after the append (native cyclone lanes are green — bridge
   tests 3/3 — so a regression would be zephyr-embedded-specific).
2. Embedded-cyclone heap rule (`ddsrt_malloc` — cyclonedds-known-limitations)
   and the `Failed to start RTPS` line — config/heap drift in the vendored
   cyclonedds fork or the zephyr conf after recent zephyr-workspace churn.
3. When was this lane last green? Its images sat unbuilt through the resync
   (the staleness gate listed them stale against orchestration-ir), so the
   breakage window is wide — bisect via the fixture build log dates.

## Repro

```
NROS_ZEPHYR_FIXTURE_FILTER=cyclonedds just zephyr build-fixtures
cargo nextest run -p nros-tests --test phase_118_collapse
```
