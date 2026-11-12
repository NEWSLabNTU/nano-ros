---
id: 2491
title: "RFC-0035's generated slot table files the loan slots and `ping_session` under \"Status events\": `rmw_vtable.h` has no section markers before them"
status: open
type: tech-debt
severity: low
area: [rmw, abi, docs]
related: [rfc-0035]
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
