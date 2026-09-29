---
id: 1575
title: "The derived-tiers-cpp Zephyr image has no lane, and without `nros sync` its configure derives no tiers and says nothing"
status: open
type: test-gap
area: [testing, zephyr, codegen]
severity: medium
found: 2026-09-29
related: [issue-1551, issue-1426, issue-1288, phase-459]
---

## What is missing

Issue 1551 was measured on `examples/workspaces/derived-tiers-cpp`'s Zephyr
entry (`native_sim/native/64`, C++, zenoh): four tiers are derived from its
SystemModel, and it was the image that ran the platform heap dry. Both of
1551's defects are fixed (#1436, #1432), and a hand-run boot shows all four
tiers ticking. **No lane boots this image**, though. It has no
`examples/fixtures.toml` row and no `matrix::CELLS` cell, so the regression
1551 fixed can come back unseen. 1551's acceptance asked for such a lane; it
closed without one, and this issue carries that item.

## The silent half

The configure names `src/demo_bringup/config/system_model.yaml`. That file is
a BUILD ARTIFACT that only `nros sync` writes (phase-330 W4.a). When it is
absent, the tier derivation returns **nothing, silently**, and the image boots
as a single executor. Its one executor's storage is already static, so a
single-executor boot proves nothing about tiers. A report that "the image
boots" on an unsynced tree is therefore evidence of nothing, and 1551's own
investigation was misled by this once (see its "Sync is load-bearing"
paragraph).

## Acceptance

- A `fixtures.toml` row for the west entry
  `examples/workspaces/derived-tiers-cpp/src/zephyr_entry`: board
  `native_sim/native/64`, `lang cpp`, `rmw zenoh`, confs
  `prj.conf;prj-zenoh.conf` plus the NSOS fragment. Its build runs `nros sync`
  on the workspace BEFORE configure.
- A `matrix::CELLS` cell, with the `lane_scope` registries kept in step. Under
  a router, it asserts that all four nodes tick (or appear in
  `ros2 node list`).
- A configure that names a SystemModel which does not exist fails and names
  `nros sync`, instead of deriving zero tiers. If some caller legitimately
  wants "no model ⇒ no tiers", that must be an explicit opt-out, not the
  fallback.
