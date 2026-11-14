---
id: 2537
title: "`[board.*]` in the SDK index is still keyed per board, not per (arch × ecosystem) profile"
status: open
type: tech-debt
severity: low
area: [boards, tooling]
related: [rfc-0064, rfc-0087, rfc-0062]
found: 2026-11-14
---

**Home: [RFC-0064](../design/0064-board-support-organization.md) Sequencing
step 6 (exploration log: "`[board.*]` is a package-set alias, not board
support")**

## What is open

RFC-0064 step 6 proposed re-keying the index's `[board.<name>]` tables as
`[profile.<arch>-<ecosystem>]`, with `[board.*]` an optional alias, so no
downstream board needs an entry in nano-ros's index. It never landed:
`nros-sdk-index.toml` has no `[profile.*]` table (checked 2026-11-14) and
still carries 15 `[board.*]` rows. The rows are still package lists that name
an architecture and an RTOS, not a board.

Part of the original motivation is gone. RFC-0087 makes an out-of-tree board
an ordinary package whose deps are its `package.xml` `<depend>` entries, so a
downstream board no longer needs an index row to be provisioned. What remains
is the in-tree duplication: every new in-tree board on an existing
(arch, RTOS) pair still adds a row.

## Found by

phase-555's design review (W2/W3, platform-board-rtos brief P7). The user
ruled (2026-11-14) that RFC-0064 becomes a short Stable RFC and that steps 6
and 8, which never landed, are filed as issues.

## What closes it

Either the profile re-key (with existing board names kept as aliases), or a
ruling in RFC-0064 that RFC-0087's package route makes it unnecessary, with
the step marked dropped.
