---
id: 2536
title: "RFC-0114's three open questions on the board registry are unanswered"
status: open
type: enhancement
severity: low
area: [boards, book, tooling]
related: [rfc-0114, rfc-0115, rfc-0064, phase-522]
found: 2026-11-14
---

**Home: [RFC-0114](../design/0114-board-support-registry-primitives.md)
"Open questions"**

## What is open

phase-522 shipped the registry (`packages/boards/board-support.toml`, the
generator `scripts/gen-board-support.py`, the `check board-registry` gate).
The RFC still lists three questions:

1. Does `nightly_token` stay an authored field, or is it derived from the
   board's `PlatformId::just_module`? phase-558 made the nightly verdict per
   board, so the token now names only the module whose verbs a board's job
   runs.
2. Should the per-board evidence page carry the numbers RFC-0115 asks for
   (time to first output, install minutes), or link to where the probe
   records them?
3. How does a downstream board package (RFC-0064's out-of-tree BSP) publish
   its own registry row, so `nros board list` shows the same status words for
   a board nano-ros does not ship?

## Found by

phase-555's design review (W3, platform-board-rtos brief P4). The user ruled
(2026-11-14) that an RFC becomes Stable only after its open questions are
filed as issues and the RFC points at them.

## What closes it

Each question ruled in RFC-0114 and, where the ruling needs code, a work item
in an open phase.
