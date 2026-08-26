# Phase 393 — what is left of the RMW contract, and what deliberately is not

**Status (2026-08-27). NOT STARTED — this doc is the ledger phase-376 did not
leave.** Phase 376 made the vtable mirror upstream and made every deviation
declared and checked. It did not say what remains, and the numbers it left
overstate delivery in one specific way that issue 0800 then measured. This is
the remainder, split into work that should proceed and reservations that should
not.

**Implements.** RFC-0054 (the C headers are the ABI SSoT). Continues phase-376
(archived) and issue 0800 (archived). Not to be confused with phase-379, which
is the USER API (rclc/rclcpp/rclrs) one layer up.

## Where the contract stands

The contract is EMPIRICAL: the 88 `rmw_*` symbols every `librmw_*_cpp.so`
defines, not the 177 the headers declare. Measured 2026-08-27 by
`just check-rmw-api-parity`:

| bucket | n | meaning |
| --- | --- | --- |
| vtable | 70 | a slot exists |
| layer | 5 | answered elsewhere, named |
| declined | 13 | deliberately absent, with an RTOS reason |
| gap | 0 | — |

`vtable` counts a SLOT, not a backend that fills one, which is the overstatement
issue 0800 found. Of those 70: **34 answered by a slot something writes and
reads, 36 by an INERT one** (no producer, no consumer). So the honest headline
is **34 of 88 working**, plus 5 at another layer, 13 declined and 0 missing.

`just check-rmw-slot-producers` keeps that split honest in both directions, and
`rmw-api-parity` now prints it beside the buckets so 70 cannot read as 70
working.

## W1 — the QoS read-back (the one CORRECTNESS item)

Issue **0823**. Six inert slots, and unlike the rest the runtime does not merely
lack the reading — it asserts the wrong one, reporting the REQUESTED QoS as
granted. Hides the most common cause of a silent ROS 2 pair.

Do this one first. It is the only item here that is a bug.

## W2 — cheap and genuinely useful

| slots | why |
| --- | --- |
| `publisher_count_matched_subscriptions`, `subscription_count_matched_publishers` | "why is nothing arriving" answered in one call; cyclone has `dds_get_matched_*` |
| `get_gid_for_publisher` | GIDs travel in the attachment today; an out-of-band read costs little |
| `get_implementation_identifier`, `get_serialization_format` | the runtime answers both; a bridge image linking two backends is where a per-backend answer stops being decoration |

Each is small, each has a live backend primitive, none changes a contract.

## W3 — reservations that should STAY reserved

Recorded so nobody spends a week closing a "gap" that is a decision. Reasons
live in `check-rmw-slot-producers.py`'s `INERT_FAMILIES` and the gate holds them.

* **graph queries + entity counts (11).** Reading the graph back means holding a
  discovered view of every peer — unbounded memory on a target. Cyclone's
  `graph.cpp` PUBLISHES this node's participant info, which is the half an
  embedded image needs; that file existing has twice been misread as these being
  implemented.
* **`publisher_wait_for_all_acked` (1).** It BLOCKS, and this ABI decomposes
  waiting into `has_data` / `drive_io` / `next_deadline_ms` precisely so one
  executor can drive several backends. A slot that blocks inside one backend
  does not fit, and that is a design property rather than a missing body.
* **on-new-* callbacks (3).** `set_wake_callback` is this ABI's answer and is
  per-SESSION, which is the shape a multi-backend executor can use.
* **with-info takes (2).** The runtime gets GID and timestamps off the
  attachment of the message it already took.
* **content filter (2), network flow (2), `feature_supported` (1).** Parity
  shape, no consumer, no demand.

## Acceptance

* `check-rmw-api-parity` still 0 gap, 0 unclassified.
* `check-rmw-slot-producers` inert count DROPS by exactly the slots W1/W2 fill,
  and the families lose exactly those names — the gate fails both ways, so a
  wired slot left in a family is caught.
* Issue 0823 has a test that fails against today's code.

## Notes for whoever picks this up

The trap in this area is that a slot's EXISTENCE reads as coverage. It caught
phase-376 (issue 0800), it caught issue 0785's enclave grouping, and it caught
`set_log_severity`, which shipped with a slot, a dispatcher and stub tests and
no backend body for two phases. Before claiming any symbol here, run
`check-rmw-slot-producers` and look at which column it lands in.
