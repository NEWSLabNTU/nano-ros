---
id: 242
title: "RMW parity gaps vs rmw.h: no publisher GID (rmw_get_gid_for_publisher) and no message-info out-param (rmw_message_info_t) at the take slot"
status: open
type: enhancement
area: rmw
related: [issue-0240]
---

## Finding (RMW/platform API audit, 2026-07-21)

Two `rmw.h` concepts have no counterpart in the nano-ros RMW vtable. Each
needs a decision: add for parity, or document as a deliberate embedded
carve-out.

1. **Publisher GID** — upstream `rmw_get_gid_for_publisher` / `rmw_gid_t`.
   No GID anywhere in the C surface (grep: no `gid`). Blocks DDS-style
   per-instance identity and bridge dedup-by-GID; the bridge currently
   dedups with a `bridge_origin` attachment instead
   (`traits.rs:1287-1310`). A GID would also give the message-info below a
   publisher identity to report.

2. **Message-info out-param** — upstream `rmw_take_with_info` fills
   `rmw_message_info_t` (source timestamp, publisher GID, reception
   sequence number). The nano-ros take slot `try_recv_raw`
   (`rmw_vtable.h:74`) returns only bytes; message info is reconstructed
   runtime-side (`lib.rs` `MessageInfoSlot`), not surfaced at the vtable.
   A `take_with_info`-shaped slot (or an out-param on `try_recv_raw`) would
   close the parity and let subscriptions observe source timestamp /
   sequence for ordering + latency measurement.

## Direction
Decide per gap. If added, both are `Option` vtable slots (NULL-able, with a
runtime fallback) consistent with the existing extension pattern — no C-ABI
break for backends that don't implement them. If carved out, note the
rationale in `book/src/design/rmw-vs-upstream.md` so it's a recorded
decision, not an unexplained absence.
