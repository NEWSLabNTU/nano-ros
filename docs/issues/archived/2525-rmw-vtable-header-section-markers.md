---
id: 2525
title: "RFC-0035's generated slot table files the loan slots and `ping_session` under \"Status events\""
status: resolved
resolution: duplicate
duplicate_of: 2491
resolved_in: "2026-11-18 (merged into issue 2491)"
type: tech-debt
severity: low
area: [rmw, c-api]
related: [phase-555]
found: 2026-11-14
---

## Resolution: duplicate of issue 2491

The same gap was already open as issue 2491, filed a day earlier from the
same phase-555 review. The facts this file added are merged into issue 2491
under "Merged from issue 2525"; track the work there.

**Home: `packages/core/nros-rmw-abi/include/nros/rmw_vtable.h` and the slot
table it generates in RFC-0035**

## What is wrong

RFC-0035's slot table takes each slot's section from the last `/* ---- … ----`
marker above it in `rmw_vtable.h`. The header has a "Status events" marker
(line 438) and no marker after it until the end, so every later slot inherits
that section. Slots 22 onward read "Status events: how the three upstream
parts map here", including the zero-copy loan slots (`borrow_loaned_message`,
line 605, and the rest of that group) and `ping_session` (line 846). None of
those is a status event.

## Why the review did not fix it

Adding the missing markers changes the C ABI header (comments only) and
regenerates the table. A docs PR should not touch the ABI header, so the
phase-555 rmw-wire review left it.

## Found by

phase-555's design review (rmw-wire brief, "Untracked work found"). The user
ruled that the untracked items be filed as issues (2026-11-14).

## What closes it

Section markers in `rmw_vtable.h` before the loan slots and before
`ping_session` (and wherever else a group starts), the table regenerated, and
`scripts/gen-abi-bindings.sh` re-run to show the bindings did not move.
