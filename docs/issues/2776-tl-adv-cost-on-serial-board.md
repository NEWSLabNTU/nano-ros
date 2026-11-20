---
id: 2776
title: "What a transient-local endpoint's `@adv` token and watch cost on a serial board, in heap and link traffic, is not measured"
status: open
type: limitation
area: [rmw-zenoh, serial, boards]
severity: low
found: 2026-11-21
related: [2366, 1806, phase-512]
---

Issue 2366 gave a transient-local publisher an `@adv` liveliness token and a
transient-local subscription a liveliness subscriber on `<topic>/@adv/pub/**`,
plus a re-query whenever a token appears. What was measured:

* every image: the subscriber slot shrinks 144 -> 112 B and the queryable
  slot grows 64 -> 72 B (x86_64; 56 / 36 B on thumbv7m), because the
  transient-local state is a heap block only a transient-local endpoint
  allocates; on the mps2-an385 bare-metal serial talker and listener the text
  grows by about 2.3 KB and the bss by 0 / -32 B;
* native heap: about +1.9 KB for a transient-local subscription and +1.0 to
  1.4 KB for a transient-local publisher (massif peak).

What was not: the heap and the link bytes of those endpoints on a serial board,
where the heap is the board's own arena and every declaration and re-query
crosses a slow UART. The safety island's `mrm_handler` subscription to
`/api/operation_mode/state` is the case that matters (issue 1806).

## Closes when

The island image (or another serial board in the board batch) is measured
with and without a transient-local subscription: arena peak, and the bytes on
the link from boot to the first delivered latched value.
