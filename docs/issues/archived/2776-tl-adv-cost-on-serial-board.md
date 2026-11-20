---
id: 2776
title: "What a transient-local endpoint's `@adv` token and watch cost on a serial board, in heap and link traffic, is not measured"
status: resolved
type: limitation
area: [rmw-zenoh, serial, boards]
severity: low
found: 2026-11-21
resolved: 2026-11-21
resolved_in: "docs(issue 2776): the transient-local @adv cost measured on the S32K344 island"
related: [2366, 1806, phase-512]
---

**Home: [phase-512](../../roadmap/phase-512-on-hardware-measurements.md)**

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

## Measured (2026-11-21)

**Board and image.** The safety island's Zephyr image on the S32K344
(MR-CANHUBK344), one serial link to the host's zenoh router. The island is
pinned to nano-ros 3c2e8cf685 and does not build against current main, so the
fix was measured as its code change (the `packages/rmw` diff of ca657d15d0)
applied to that pin, against the same pin without it. The image has three
transient-local endpoints: `mrm_handler`'s subscription to
`/api/operation_mode/state`, and the comfortable-stop operator's two publishers
(`max_velocity_candidates`, `clear_velocity_limit`). Those two are the
board-side transient-local publisher case; no separate image was needed.

**Method.** A host rclpy node (rmw_zenoh_cpp, client of the router) publishes
one `OperationModeState` sample, transient-local, depth 1, and stays up. The
board is then hardware-reset over SWD with its serial counters zeroed. Every
2 ms the probe reads the zenoh-pico serial counters and a stamp that the
handler's first callback writes (board uptime). The heap peak and arena come
from the boot report read after 20 s. Each variant changes both the call site
and its contract row, because the declared-QoS check refuses a volatile call
site under a transient-local row. Each variant ran 3 to 5 times. Every byte
count and heap peak repeated exactly; only rx at the first delivery varied, by
up to 24 B (the poll landing mid-frame).

| variant | fix | op-mode sub | 2 stop pubs | heap peak (B) | first latched delivery | link at 1.1 s, tx / rx (B) | link at 2 s, tx / rx (B) |
| --- | --- | --- | --- | --- | --- | --- | --- |
| A | yes | transient-local | transient-local | 69,896 | 1.111-1.120 s | 8,847 / 661-685 | 10,980 / 936-950 |
| B | yes | volatile | transient-local | 68,032 | none (volatile) | 8,685 / 278 | 10,708 / 278 |
| C | no | transient-local | transient-local | 65,992 | 1.107-1.113 s | 8,300 / 496-515 | 10,388 / 515 |
| F | no | volatile | transient-local | 65,832-65,992 | none (volatile) | 8,308 / 278 | 10,331 / 278 |
| D | yes | volatile | volatile | 63,464 | none | 7,871 / 278 | 9,894 / 278 |
| E | no | volatile | volatile | 63,464 | none | 7,871 / 278 | 9,894 / 278 |

The executor arena is 12,160 of 22,352 B in every variant. Delivery is about
1.11 s after reset in A and C alike; the fix adds no latency to the first
latched value. "1.1 s" is the delivery instant. Tx after it grows with the
handler's own periodic output, the same in every variant.

**What the fix costs on this board** (differences of the rows above):

| cost | heap peak | tx to delivery | rx |
| --- | --- | --- | --- |
| transient-local subscription, with the fix (A - B) | +1,864 B | +162 B | +383 to +407 B by delivery, +658 to +672 B by 2 s |
| transient-local subscription, without the fix (C - F) | +0 B | -8 B | +218 to +237 B (the one reply) |
| the fix's share of the subscription | +1,864 B | +170 B | +146 to +189 B by delivery, +421 to +435 B by 2 s |
| two transient-local publishers, with the fix (B - D) | +4,568 B | +814 B | 0 |
| two transient-local publishers, without the fix (F - E) | +2,528 B | +437 B | 0 |
| the fix's share of the two publishers | +2,040 B (about 1 KB each) | +377 B (about 190 B per `@adv` token) | 0 |
| the fix on the whole island image (A - C) | +3,904 B | +547 B | +146 to +189 B by delivery, +421 to +435 B by 2 s |
| the fix on an image with no transient-local endpoint (D - E) | 0 | 0 | 0 |

Bytes from boot to the first delivered latched value on the island image are
8,847 B tx + 661-685 B rx with the fix, against 8,300 B + 496-515 B without it.

The fix's rx after delivery is the re-query: when the subscription's watch sees
the host publisher's `@adv` token, it queries the cache again. The latched
sample (about 420 B with its reply framing) crosses the link a second time
between 1.1 and 2 s, and the callback still runs once (`count` = 1 in every A
run). The cost is one extra history reply per token that appears, not a
periodic one.

The heap peak moves as well as grows. Without the fix, the island's peak
comes after the first spin (45,136 B at FirstSpin, then +20,856 B). With it,
the peak is already reached by FirstSpin (69,664 B, then +232 B).

**Image size** (`arm-zephyr-eabi-size -A`, same pin, with the fix minus
without):

| image | .text | .rodata | .bss | .data |
| --- | --- | --- | --- | --- |
| island, 3 transient-local endpoints (A - C) | +1,808 B | +184 B | -216 B | 0 |
| island, none (D - E) | +1,800 B | +184 B | -224 B | 0 |

These agree with the mps2-an385 serial images in issue 2366 (+2.3 KB text,
0 / -32 B bss). The bss falls because the transient-local state left the
subscriber slot for the heap.

**Headroom.** The island's heap is 94,720 B with a 24,576 B headroom floor.
Without the fix the island peaks at 65,992 B (28,728 B spare); with it at
69,896 B (24,824 B spare), 248 B above the floor. A pin bump that brings the
fix into the island still passes the boot report's headroom check, but by
248 B. Any later heap growth on the island fails it unless
`CONFIG_NROS_ZEPHYR_HEAP_SIZE` grows.

## Resolution

Measured as the closing condition asks, on the island image, with and without
a transient-local subscription. On the board the fix costs the same as on the
native host:

* about 1.9 KB of heap per transient-local subscription;
* about 1 KB of heap per transient-local publisher;
* about 190 B on the link per `@adv` token declaration;
* one extra history reply when a publisher's token appears.

It adds no latency to the first latched value. An image without
transient-local endpoints pays only about 1.8 KB of text. The island's heap
headroom with the fix is 248 B above its floor; that is the island's sizing
input, not a defect here.
