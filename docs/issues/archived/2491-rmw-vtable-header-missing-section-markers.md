---
id: 2491
title: "RFC-0035's generated slot table files the loan slots and `ping_session` under \"Status events\": `rmw_vtable.h` has no section markers before them"
status: resolved
resolved: 2026-11-19
resolved_in: "the change that archives this issue"
type: tech-debt
severity: low
area: [rmw, abi, docs]
related: [rfc-0035, 2525, phase-555]
found: 2026-11-13
---

**Found by** the phase-555 W2 review of RFC-0035. Listed under "untracked
work" in `docs/roadmap/data/phase-555-w2-rmw-wire-brief.md`.

## Measured

- `scripts/gen-rmw-slot-table.py` groups the slots in RFC-0035's table by the
  last `/* ---- … ---- */` marker above each slot in
  `packages/core/nros-rmw-abi/include/nros/rmw_vtable.h`.
- The last marker before slots 22-35 is "Status events: how the three
  upstream parts map here" (around `rmw_vtable.h:438`). There is no marker
  before the loan slots (around :605) or `ping_session` (around :846). So the
  table files `publisher_take_event` through `ping_session` together under
  status events.
- `gen-rmw-slot-table.py --check` passes, because the table is consistent
  with the header. The header's grouping is what is wrong.

## Next

Add the missing markers (loans, session liveness) in `rmw_vtable.h`, then
regenerate the table. This changes comments only, but it touches the C ABI
header, so run `just check abi-bindings` and confirm the committed bindgen
output does not move. That is why the review PR did not make the change.

## Merged from issue 2525 (2026-11-18)

Issue 2525 was filed for the same gap a day later, from the same phase-555
review. It is archived as a duplicate of this one. What it added:

- The missing markers go wherever a group starts, at least before the loan
  slots (`borrow_loaned_message`, around line 605) and before `ping_session`
  (around line 846).
- Closing evidence: the table regenerated, and `scripts/gen-abi-bindings.sh`
  re-run to show the committed bindings did not move.

## Resolved (2026-11-19)

The table was wrong in two places, not one.

- **Missing markers.** Five section markers are added in `rmw_vtable.h`:
  executor wake and next-deadline hints (25-26), zero-copy loans (27-31),
  service-server availability (32), batched take and streamed publish
  (33-34), and session liveness (35). Slots 25-35 had all been filed under
  status events.
- **A false marker.** The explanatory block inside the status-events
  section began `/* ---- Status events: how the three upstream parts map
  here ----`, which the generator read as a section of its own. It split
  slot 20 from slots 21-24. That block is now an ordinary comment, so all
  five event slots group under "Status events".

`scripts/gen-rmw-slot-table.py` regenerated RFC-0035's table (`--check` OK,
78 slots). Comments only: `just check abi-bindings` reports "ABI bindings
match the C-header SSoT (bindgen 0.72.1, libclang 14)", so the committed
bindgen output did not move.
