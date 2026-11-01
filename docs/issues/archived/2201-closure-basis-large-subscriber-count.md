---
id: 2201
title: "`NROS_MAX_LARGE_SUBSCRIBERS` was derived as a TYPE count when an image declares no subscriptions"
status: resolved
resolved_in: "2026-11-01"
type: bug
area: [cmake, rmw-zenoh, memory]
severity: medium
found: 2026-11-01
related: [0940, 0963, 1122, 1125, 2202]
---

## Problem

`nros_derive_message_bound_knobs()` (`cmake/NanoRosMessageBounds.cmake`) and
its Rust twin (`nros_cli_core::message_bound_knobs`) derive the three zenoh
payload-class knobs on one of two bases. When the image declares its entities,
the basis is `subscribed`, and `NROS_MAX_LARGE_SUBSCRIBERS` is the number of
subscribing ENTITIES whose type is over the class split. That is correct.

When the image declares none (no `[[component]] entities`, no launch contract,
or the first configure's placeholder fragment), the basis is `closure`. There
the count was `list(LENGTH _large_types)`, the number of LINKED TYPES over the
split. The code called this "an upper bound". It is not a bound in either
direction:

* a talker that subscribes to nothing, linking one large type, was given
  `1 x RING_DEPTH x LARGE_SIZE` of `LARGE_PAYLOADS` for a class it never
  routes into;
* an image with three subscriptions on one large type was given one block, and
  the second and third `create_subscription` fail.

The two SIZES on that basis are genuine upper bounds (no received sample can
be larger than the largest linked type). The COUNT is a fact about entities
the image did not state, so it was a guess presented as `derived`.

## Found by

A C zenoh talker on the 520 KiB `xiao_rp2350` (issue U4 of the 2026-11-01
spike). Its first link failed with ``region `RAM' overflowed by 57240
bytes``, and the workaround was `[image.*.knobs.zenoh] max_large_subscribers =
0`.

The spike's diagnosis ("derive (-1) counted the String type") was not right
for that image, and the measurement shows why. Reproduced on this tree with
the spike's app and `-DZEPHYR_EXTRA_MODULES=<this checkout>`:

| talker `system.toml` | `NROS_MAX_LARGE_SUBSCRIBERS` | RAM |
|---|---|---|
| no `entities` | crate default 2 (refused: 15 of 32 linked `std_msgs` types unbounded) | link fails, `RAM overflowed by 176 bytes` (532,656 B of 532,480) |
| `entities = ["publisher:std_msgs/msg/String:/chatter", "timer"]` | **0, derived** over the subscribed set (issue 0963) | **249,392 B** (46.8 %) |

The xiao image never reached the closure basis: `std_msgs` has unbounded
types, so the whole bound derivation refused and the knob fell to the crate
default (`2 x 4 x 16384 = 131,072 B`). A refusal falling to the configured
value is the rule, and it is what this image got. What was wrong was the
type-count path, which is reached whenever the closure IS bounded (capped
interfaces, or a closure of fixed-size types) and nothing declares the
entities.

## Fix

* On the `closure` basis the large COUNT is published only when it is `0`,
  meaning no linked type is over the split. Then nothing the image could receive
  routes large, whatever it subscribes to. Any other closure leaves
  `NROS_DERIVED_MAX_LARGE_SUBSCRIBERS` unset, so the knob keeps its
  configured value. The written fragment says so in a comment, and the
  configure prints `NROS_MAX_LARGE_SUBSCRIBERS not derived -- N linked type(s)
  are over the ... ceiling and this image declares no entities`.
* The large SIZE and the small size are still published on that basis, because
  both are upper bounds.
* The `subscribed` basis is unchanged.
* The Rust twin makes the same change. The seed must stay byte-identical, and
  `tests/cmake-message-bounds-tests.sh` case P checks this on every case.
* The xiao case gets a pointer to its real remedy. When an undeclared zenoh
  image is refused for an open closure, the configure now says that declaring
  its entities derives the payload classes anyway. It is not said on a clean
  build dir's placeholder pass, where a declaring image still reads as
  undeclared.

## Tests

* `tests/cmake-message-bounds-tests.sh` case A (large half): a bounded closure
  with two types over the split and no entities publishes NO count, in either
  the variables or the answer file, and reports why. It used to assert `2`.
  Case C now also asserts the remedy line. With the cmake change reverted, the
  three A assertions and the parity check fail.
* `message_bound_knobs::tests`: `the_closure_basis_does_not_derive_the_large_count`
  (no fragment, and a `refused` fragment),
  `a_closure_with_nothing_large_still_answers_zero`, and
  `the_subscribed_basis_counts_large_subscriptions` (3 on one type gives 3,
  and an empty set gives 0).

## Not in scope

The other big-RAM defaults the same image showed (heaps, executor storage,
service inbox, stacks) are issue 2202.
