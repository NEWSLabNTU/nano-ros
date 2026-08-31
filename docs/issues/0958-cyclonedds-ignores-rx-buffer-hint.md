---
id: 958
title: "The Cyclone RMW discards `rmw_subscription_options_t` entirely, so no
  per-type receive sizing reaches a Cyclone image"
status: open
type: bug
area: rmw
related: [issue-0896, issue-0917, phase-392, phase-408]
---

## What was measured

`subscription_create` in the Cyclone backend takes the options struct and names
none of it:

```cpp
rmw_ret_t subscription_create(const rmw_node_t* node, const char* topic_name,
                                 const char* type_name, const char* /*type_hash*/,
                                 uint32_t /*domain_id*/, const rmw_qos_profile_t* qos,
                                 const rmw_subscription_options_t* /*options*/,
                                 rmw_subscription_t* out) {
```

`grep -rn rx_buffer_hint packages/rmw/cyclonedds/` returns nothing. The field is
declared in the ABI (`nros-rmw-abi/include/nros/rmw_entity.h`), read by
`rust_adapter`, produced by the Rust executor since phase-392 W3a and by the
C++ register variants since phase-402 — and then dropped on the floor by this
backend.

## Why it matters

Every consumer-visible mechanism for sizing a receive buffer from the message
type currently terminates in the zenoh shim's `alloc_payload_block(hint)`. A
Cyclone image gets none of it:

* phase-392 **W3a** (Rust routes the block by the type's bound) — no effect.
* phase-402 / issue **0896** (C++ options struct carries the hint) — no effect.
* phase-408 W1/W4 (emit the constant, deliver it from a generated helper) — no
  effect on the backend half.

So a Cyclone consumer can do everything the campaign asks and observe nothing.
That is worse than an unimplemented feature: the knobs exist, the docs describe
a saving, and the measurement comes back flat with no indication that the
backend is where it stopped.

**This is not hypothetical.** The autoware-safety-island an536 lane is a Cyclone
image; it is the consumer that motivated issues 0896 and 0917, and it is the one
that cannot benefit from either until this is closed.

## What is NOT affected

The executor's own arena is sized by nano-ros, not by the backend
(`arena_alloc_with_trailing` + `buffered_region_size`), so phase-408 W3 —
demoting `RX_BUF` to a runtime argument — shrinks a Cyclone image's arena
regardless of this issue. The two are independent, and conflating them will
produce a measurement that appears to contradict whichever lands first.

The rule for anyone measuring phase-408 on a Cyclone lane: **measure the arena,
not the backend.**

## What "fixing" means, and the honest alternative

Cyclone's own receive path is not a two-class static pool like zenoh-pico's, so
"route the block by the hint" does not transliterate. Two acceptable outcomes,
and the issue is closed by EITHER:

1. **Consume it** — establish what, if anything, the hint should change in this
   backend (a `Sizing/ReceiveBufferSize`-adjacent knob, a per-reader
   `dds_set_qos` history bound, or a defrag budget), and wire it.
2. **Declare it unsupported, in the backend's own docs**, so a consumer stops
   looking for an effect that cannot occur. `rmw_subscription_options_t` is a
   shared ABI; a backend that ignores a field should say so where the field is
   read, not leave the reader to `grep` for the absence.

Option 2 is a legitimate close. What is not legitimate is the present state,
where the parameter is silently discarded at a comment.

## Not to be confused with

Issue **0841** — a hint that exists landing in a size class that cannot hold it.
That is about routing a hint. This is about a backend that never sees one.

Issue **0917** — the an536 fragment cliff, which is an RX FIFO capacity limit in
the emulated NIC and is unrelated to buffer sizing above the driver.
