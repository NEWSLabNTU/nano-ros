---
id: 2853
title: "A message over 64 KiB on the POSIX zenoh build needs the fragment and batch sizes raised by hand; they are not derived from the declared message sizes"
status: resolved
type: limitation
severity: medium
area: [rmw, zenoh, posix, sizing]
related: [phase-589, issue-1368, issue-2942, issue-2943]
found: 2026-11-23
resolved: 2026-11-24
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

## Ruling (2026-11-24, user, via the manager)
Issue 1368 left the reassembly ceiling underived on purpose: fragmentation
is a transport and link property, the buffer is one allocation of the whole
ceiling from the platform arena, and a closure bound over-states what an image
receives. The ruling keeps that rule and gives it a scope:

- On a HOSTED POSIX board the ceiling is derived, as a rung below every
  stated value, from the image's RECEIVE class (the payload class its largest
  subscription routes into), never from the whole closure: max(the builtin,
  that class plus the header headroom). The buffer there comes from the
  process heap, so 1368's arena reason does not apply.
- Embedded boards keep 1368's refusal.
- An image that receives a type with no size bound and states no receive
  class gets a build WARNING naming the knob and saying oversized samples are
  dropped, not a failure.

Recorded beside 1368's reasoning in `receive_ceiling.rs`'s module doc, so the
two read as one rule with a scope.

## Resolution
- `nros_sizing_descriptor::receive_ceiling::stated_receive` is the ONE
  computation of an image's stated receive need (the stated knob, else the
  descriptor's payload class), used by the refusal in `nros-rmw-zenoh`'s
  build script and by the derivation in the zpico runner.
- `hosted_frag_max` derives the ceiling as the class plus
  `REASSEMBLY_HEADROOM_BYTES` (256). That headroom was measured, not assumed:
  a 100,012 B class over a local `rmw_zenohd` needs about 88 B of zenoh
  headers on top. A 100,095 B ceiling delivered nothing; 100,100 B delivered
  every sample. The runner applies it only on a POSIX, non-Zephyr build, and
  only when no rung stated `frag_max_bytes`.
- `unpriced_receive_warning` replaces the old refused-classes warning, which
  named retired `NROS_DECLARED_*` carriers. It fires only when no class was
  stated, on both roads.

Measured on the Linux talker/listener (zenoh client to a local router):

| Case | Ceiling baked | Result |
| --- | --- | --- |
| `uint8[100000]` message, no zenoh knob | 100,268 (derived) | 29 of 29 delivered |
| same, `frag_max_bytes = 65535` stated | 65,535 | refused at build, as before |
| `std_msgs/String` listener, nothing stated | 65,535 | builds, warns |
| same, `subscriber_large_bytes = 200000` stated | 200,256 (derived) | builds, no warning |

Two defects found on the way, filed separately:
- 2942: the metadata probe compiles a component with the default executor
  subscription buffer, so a bounded message over 1024 B fails the probe
  before the image's derived buffer applies. The acceptance run set
  `NROS_EXECUTOR_SUBSCRIPTION_BUFFER_BYTES` by hand for that reason; no zenoh
  knob was set.
- 2943: the refusal compares the class without the header headroom, so a
  class within about 88 B of the ceiling passes and drops every sample.
