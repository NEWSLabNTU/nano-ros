---
id: 2570
title: "`ftti_budget` charges only the last level of a mode-form hazard's fallback ladder, while play_launch judges every level"
status: resolved
type: bug
area: [cli, contract]
severity: medium
found: 2026-11-13
related: [phase-560]
resolved: 2026-11-14
resolved_in: "74ed310d1b feat(ftti_budget): charge every ladder level (#3758, #3759) (#3853)"
---

GitHub: NEWSLabNTU/nano-ros#3758

**Home: [phase-560](../roadmap/phase-560-contract-revision-follow.md)**, which
follows the contract revision on the RTOS side (W1 fixed which level is
charged; W8 made the image report its level). L4 plan item NR-15 (gaps G18,
G25). rlm side: NEWSLabNTU/ros-launch-manifest#6.

**Resolved:** PR #3853 merged 2026-11-14 (`74ed310d1b`); GitHub #3758 closed by it.

## What happens

For a mode-form hazard, the build-time `ftti_budget` charges only the last
level of the fallback ladder. play_launch judges every level. On an L4 Safety
Island contract the two runners therefore report different binding rows:

- play_launch: B3 at 80 s;
- nano-ros: B4 at 21 s.

## Direction

- Charge each level that reaches a settle against the hazard's FTTI, and
  against the level's own deadline once rlm has `modes.<m>.within`.
- Have the running image report which ladder level it is in, so an act can
  compare it with play_launch. Phase-560 W8 (rule `hazard-level-runtime`,
  2026-11-14) states that the image reports each level its ladder becomes
  active on; confirm it covers this half when the build-time half lands.
