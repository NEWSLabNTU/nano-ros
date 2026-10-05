---
id: 1709
title: "A zenoh transient-local publisher retains ONE sample whatever it declares —
  `TL_RETAIN_DEPTH` is a constant, not a derived knob"
status: open
type: limitation
area: [rmw]
severity: low
found: 2026-10-06
related: [1687, phase-480]
---

## What is true today

`nros-rmw-zenoh/build.rs` emits `pub const TL_RETAIN_DEPTH: u32 = 1;`, and
`shim/qos.rs::admit` grants a transient-local publisher at most that depth and
advertises the grant in the liveliness token (phase-455 W5). So a publisher that
declares `TRANSIENT_LOCAL` + `KEEP_LAST(10)` is advertised to a stock peer as
`KEEP_LAST (1)`, and a late joiner receives one sample, not ten. The grant is
truthful and the boot log says so once:

    qos: publisher '/qos_chatter' asked for TRANSIENT_LOCAL KEEP_LAST(10); this
    backend retains 1 sample and replays it on a late joiner's query. Granting 1
    and advertising it to the graph.

Measured 2026-10-06 on `native_rust_qos` / `native_c_qos_talker` against
`rmw_zenohd` (Humble) with `ros2 topic info -v` — issue 1687.

## Why it is filed

Issue 1687's four red cells were exactly this number: the demos declared
KEEP_LAST(10), the wire said 1. 1687 was fixed by making the demos declare the
depth this backend serves. The limitation itself is unchanged: an application
that needs the last N samples latched (a map, a parameter snapshot history)
cannot get it on zenoh, while `subscriber_ring_depth` is already DERIVED from
the largest declared subscription depth (phase-454 W6.a).

## Shape of a fix

`shim/publisher.rs::transient_local` names it as the extension point: make the
depth a knob, derived like the subscriber ring from the largest declared
transient-local publisher depth in the sizing descriptor, and keep a ring of
that many retained samples per `RetainSlot`. The pool is priced
(`MAX_TL_PUBLISHERS × (TL_RETAIN_BYTES + …)`), so the cost multiplies by the
depth and must be reported the way the other derived pools are. The query
callback then replies once per retained sample, oldest first.

## Acceptance

A transient-local publisher declaring KEEP_LAST(N) within the derived bound is
advertised as KEEP_LAST(N), and a late-joining stock subscriber receives N
samples.
