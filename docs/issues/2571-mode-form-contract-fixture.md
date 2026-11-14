---
id: 2571
title: "No contract or test declares `hazards:` with a `modes:` ladder, so judging rungs on an image has never run against a real ladder"
status: open
type: test-gap
area: [testing, contract]
severity: medium
found: 2026-11-13
related: [phase-560, 2570]
---

GitHub: NEWSLabNTU/nano-ros#3759

**Home: [phase-560](../roadmap/phase-560-contract-revision-follow.md)**. L4
plan item NR-19 (gap G25). Depends on issue 2570 (the per-level
`ftti_budget`).

**In flight:** PR #3853 (branch `feat/3758-ftti-ladder-levels`, "Closes
#3759"), open and not merged as of 2026-11-15. This issue stays open until
it lands.

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
