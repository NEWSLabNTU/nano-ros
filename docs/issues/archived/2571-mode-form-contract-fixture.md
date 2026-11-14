---
id: 2571
title: "No contract or test declares `hazards:` with a `modes:` ladder, so judging rungs on an image has never run against a real ladder"
status: resolved
type: test-gap
area: [testing, contract]
severity: medium
found: 2026-11-13
related: [phase-560, 2570]
resolved: 2026-11-14
resolved_in: "74ed310d1b feat(ftti_budget): charge every ladder level (#3758, #3759) (#3853)"
---

GitHub: NEWSLabNTU/nano-ros#3759

**Home: [phase-560](../roadmap/phase-560-contract-revision-follow.md)**. L4
plan item NR-19 (gap G25). Depends on issue 2570 (the per-level
`ftti_budget`).

**Resolved:** PR #3853 merged 2026-11-14 (`74ed310d1b`); GitHub #3759 closed by it.

## What is missing

No nano-ros contract or test declares `hazards:` with a `modes:` ladder, so
judging rungs on an image has never run against a real ladder.

## Done when

A reduced L4 ladder is a fixture with:

- 3-4 modes with `requires`/`fallback`;
- one dwell;
- two hazards with `enters:`.

The build-time rows and the run-time level report (phase-560 W8) are tested
against it.
