---
id: 242
title: "RMW parity gaps vs rmw.h: no publisher GID (rmw_get_gid_for_publisher) and no message-info out-param (rmw_message_info_t) at the take slot"
status: resolved
type: enhancement
area: rmw
related: [issue-0240]
---

## RESOLUTION (2026-07-27) — carve out both, documented; functionality already exists

Decided per the issue's "add for parity vs carve out" framing: **carve out
both**, because the underlying functionality is ALREADY present in nano-ros's own
shape — only the upstream-exact C entry points are not mirrored. The audit's
premise (the data is absent) was outdated:

- **Message-info** is surfaced to applications via the `message_info()`
  subscription builder (Rust: `node.subscription(t).message_info().build(|msg,
  Option<&MessageInfo>|…)`; C: `nros_executor_register_subscription_raw_with_info`).
  `MessageInfo` already carries all five `rmw_message_info_t` fields (both
  timestamps, both sequence numbers, publisher GID). What is carved out is only
  the `take_with_info`-shaped VTABLE slot — the metadata rides a `MessageInfoSlot`
  side-channel so the hot `try_recv_raw` byte-count path stays lean.
- **Publisher GID** IS carried on the wire where supported: a zenoh publisher
  generates a 16-byte GID (`RmwAttachment::generate_gid`) and stamps it into the
  per-sample attachment; the subscriber populates `MessageInfo.publisher_gid`, so
  per-message attribution is observable via `info.publisher_gid()`. What is carved
  out is only the standalone `rmw_get_gid_for_publisher` QUERY — no in-tree
  consumer needs it (bridge dedup uses the `bridge_origin` attachment).

Recorded in `book/src/design/rmw-vs-upstream.md` (the "Publisher GID &
message-info" section — corrected from the earlier stale "no GID / never surfaced
to the app" text). Both upstream-exact shapes can still land later as NULL-able
optional vtable slots (RFC-0035 tail-append) the day a concrete consumer needs
them. No code change — the functionality already ships.

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
