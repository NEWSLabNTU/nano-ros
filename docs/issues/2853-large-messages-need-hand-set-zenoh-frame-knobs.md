---
id: 2853
title: "A message over 64 KiB on the POSIX zenoh build needs the fragment and batch sizes raised by hand; they are not derived from the declared message sizes"
status: open
type: limitation
severity: medium
area: [rmw, zenoh, posix, sizing]
related: [phase-589, issue-1368]
found: 2026-11-23
---
Found by a downstream proof of concept on an embedded Linux camera SoC (HiSilicon Hi3519DV500: 2x Cortex-A55, Linux 5.10, musl 1.2.3, BusyBox), built as an out-of-tree hosted board; see phase-589.

## What
The proof of concept sent 100 kB strings at 30 Hz (1950 of 1950 delivered,
23.6 Mbit/s, 4.8 % CPU) after setting by hand a 262144-byte fragment maximum,
one large subscriber, and a 131072-byte large-subscriber buffer.

On a POSIX build zenoh's fragment and unicast batch sizes are literal builtins,
65535 each (`nros-zpico-build` `runner.rs`). The large-subscriber class
(`NROS_ZENOH_SUBSCRIBER_LARGE_BYTES`, `NROS_ZENOH_MAX_LARGE_SUBSCRIBERS`) is
derived from the linked closure's declared sizes, but `zenoh.frag_max_bytes`
and `zenoh.batch_unicast_bytes` are not. Issue 1368 made a sample larger than
`max(frag, batch)` a build refusal, but only for classes that state their size;
an unbounded `string` states none, so the drop on a SUCCESS return that 1368
describes is still reachable.

## Fix direction
Derive the fragment size from the largest stated message the image sends or
receives, and keep the refusal for a sample that cannot fit. For an unbounded
type, require the size to be stated rather than taking the builtin silently.

## Acceptance
An image whose declared messages exceed 64 KiB builds with no hand-set zenoh
knob and delivers them; one with an unbounded type is told what to state.
