---
id: 2536
title: "RFC-0114's three open questions on the board registry are unanswered"
status: open
type: enhancement
severity: low
area: [boards, book, tooling]
related: [rfc-0114, rfc-0115, rfc-0064, phase-522, phase-558]
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

## Ruling (2026-11-17)

The user accepted all three recommendations of the board-support decision
brief, as written. They are recorded in RFC-0114's "Open questions" and its
decision log.

1. **`nightly_token` is derived.** Measured on main: 10 rows carry the
   token. All 9 that name a `matrix_platform` already equal
   `PlatformId::just_module` of that platform. The tenth
   (`nros-board-mps3-an536-freertos`) names no `matrix_platform`, so there
   is nothing to derive from, and it keeps an authored value. The field
   stays legal only on a row with no `matrix_platform`. The work is
   [phase-558](../roadmap/phase-558-nightly-per-board-verdicts.md) W8.
2. **The evidence page links to the probe's numbers, once the probe records
   them.** Measured: the setup probe (`scripts/probe/`) records no timings
   today, so there is nothing to link. Until it does, the page shows no
   number. It never shows a hand-copied one. **Trigger:** the probe records
   time to first output and install minutes per board.
3. **A downstream board package carries its own registry row,** and its
   status can claim no more than its own CI proves. Measured: `nros board
   list` reads only `packages/boards/board-support.toml`, and no board
   package outside the tree exists yet. So the shape is decided now and the
   merge is built later. **Trigger:** the first out-of-tree board package
   that wants a row.

## What closes it

Phase-558 W8 lands (question 1), and the two triggers above have fired and
their work has landed (questions 2 and 3). Until then this issue stays open
for those two, by design.
