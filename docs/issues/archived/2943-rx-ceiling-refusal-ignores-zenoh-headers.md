---
id: 2943
title: "The receive-ceiling refusal passes a class equal to the ceiling, but the reassembled zenoh message is ~88 B larger, so every such sample is dropped"
status: resolved
type: bug
severity: low
area: [rmw, zenoh, sizing]
related: [issue-1368, issue-2853]
found: 2026-11-24
resolved: 2026-11-24
---
## What
`receive_ceiling_refusal` (`nros-sizing-descriptor/src/receive_ceiling.rs`)
and its Zephyr twin (`_nros_rx_ceiling`, `zephyr/cmake/nros_cargo_build.cmake`)
refuse only when the receive class EXCEEDS `max(frag, batch)`. But zenoh-pico
reassembles the whole zenoh message, which is the sample plus the push header,
its extensions and the RMW attachment. Measured for issue 2853 (Linux, a
100,012 B class over a local `rmw_zenohd`): a 100,095 B reassembly ceiling
delivered nothing and 100,100 B delivered every sample, so about 88 B of
headers. A class in `(ceiling - ~88, ceiling]` passes the check and every
sample is dropped silently, the failure 1368 exists to prevent.

The hosted derivation from 2853 already adds `REASSEMBLY_HEADROOM_BYTES`
(256). The refusal does not, on either road.

## Fix direction
Compare `need + REASSEMBLY_HEADROOM_BYTES` against the ceiling in both
arms. This can newly refuse an embedded image whose class sits within the
headroom of its ceiling, so measure the overhead on an embedded road (the
batch path too) before changing it there.

## Acceptance
An image whose class is within the headroom of its ceiling is refused with the
same message, on both roads; one with room passes.

## Resolution
Both arms now compare the class plus `REASSEMBLY_HEADROOM_BYTES` (256)
against `max(frag, batch)`: `receive_ceiling_refusal` (every road where
`zpico-sys` compiles zenoh-pico) and `_nros_rx_ceiling` (the Zephyr
configure, `set(_nros_rx_headroom 256)`). The refusal prints the size with
headers and asks for at least that. The headroom is the same in both:
`the_zephyr_arm_states_the_same_headroom` reads the cmake literal and fails
when the two disagree.

On measuring the embedded road first: the overhead is the zenoh message's
own framing (push header, its extensions, the RMW attachment). It comes from
zenoh-pico's protocol code, which is the same on every platform, so the hosted
measurement stands for it. The batch arm gets the same headroom, since a
batch carries the same message.

Tests: a class with its headers at the ceiling passes; a class equal to the
ceiling is refused, naming the size with headers; the batch case moved by the
headroom.

Not swept: the in-tree embedded images' resolved classes are not cached in a
form a cheap survey could read. An image whose class sits within 256 B of its
ceiling now fails at configure, naming the knob. The tier 2 matrix, which
builds them, was not run for this change.
