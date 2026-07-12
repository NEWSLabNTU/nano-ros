---
id: 173
title: "Zephyr (native_sim) publisher → native subscriber delivers nothing through a shared zenoh router"
status: resolved
type: bug
area: rmw
related: [issue-0164, phase-286, issue-0147]
---

## RESOLVED — stale-fixture false alarm (not an RMW bug) — 2026-07-12

The "zenoh-pico pub → native sub broken" premise was a **misdiagnosis caused by a
stale native-listener fixture**. The prebuilt `examples/native/rust/listener`
binary was **Int32-era (built 07-01)** while its source had migrated to
`std_msgs/String` on **07-06**. rmw_zenoh bakes the message type into the wire
keyexpr (`0/chatter/std_msgs::msg::dds_::<Type>_/*`), so the stale **Int32**
listener subscribed `…::Int32_/*` while every talker (rust + cpp, zephyr + native)
publishes `…::String_/TypeHashNotSupported` → the router never matched the two
keyexprs → 0 delivery.

Why it looked like a Zephyr-pub bug: the #164 sweep / phase-286 W1 cross tests run
with `NROS_SKIP_FIXTURE_CHECK=1` (the parallelism work bypassed the guard), which
ran the **stale** Int32 listener instead of erroring on it. The content-aware
staleness guard (#147 / phase-286 W2) DOES catch it when not skipped (a fresh run
errors `Test fixture is STALE — a source is newer than the built binary`).

**Diagnosis path:** captured the wire keyexprs with `zenohd` debug logging — sub =
`…Int32_/*`, pub = `…String_/…`. Rebuilt the listener from current source (now
`String`); re-ran in the REAL harness (not a hand-rolled repro, which is itself
unreliable — a manual native→native repro also shows 0 while `nano2nano`'s
`test_talker_listener_communication` PASSES). **All four #173 cross lanes now PASS:**
`test_zephyr_to_native_e2e` (45 s), `test_bidirectional_native_zephyr_e2e` (10 s,
both directions), `test_zephyr_cpp_talker_to_native_listener` (4.5 s),
`test_native_talker_to_zephyr_cpp_listener` (9 s).

**Fix = rebuild the fixture** (no code change). Lesson: `NROS_SKIP_FIXTURE_CHECK`
masks genuine staleness — the #164 cross-test "delivery" failures were stale-fixture
artifacts, not RMW bugs. Do not skip the guard on delivery-class e2e.

## Problem

A Zephyr native_sim zenoh-pico **publisher** does not deliver to a **native**
(host `nros`) subscriber through a shared host `zenohd`, while the reverse
direction works. Exposed by phase-286 W1 slice 3 (per-test ephemeral routers let
the rust pubsub cross-tests run past the staleness guard and reach real delivery).

## Evidence (2026-07-09)

Same host `zenohd`, both peers dialing it:

- **`test_zephyr_to_native_e2e`** — Zephyr talker → native listener: native
  listener logs **0** `Received:` lines. Fails **serially** (40 s), so it is NOT
  a parallelism / port-collision artifact.
- **`test_bidirectional_native_zephyr_e2e`** — one router, both directions at
  once: **Native→Zephyr = 41** samples delivered, **Zephyr→Native = 0**. The
  asymmetry is inside a single router instance.
- **`test_zephyr_talker_to_listener_e2e`** (Zephyr↔Zephyr) — **passes**.
- **`test_native_to_zephyr_e2e`** — native talker → Zephyr listener — the Zephyr
  subscribe side works (the 41 above).

So the broken path is specifically **zenoh-pico publisher (Zephyr native_sim) →
host zenohd → native (rust `nros`) subscriber**. Zephyr-as-subscriber and
Zephyr↔Zephyr both work; only Zephyr-as-publisher-to-a-native-sub fails.

**C++ is worse (phase-286 W1 slice 2, 2026-07-09).** When the C++ pubsub cross
tests were converted to per-test ephemeral routers, BOTH directions failed for
the C++ zephyr peer: `test_zephyr_cpp_talker_to_native_listener` (cpp-pub →
native-sub, the same class as the rust case above) AND
`test_native_talker_to_zephyr_cpp_listener` (native-pub → **cpp-sub**, 0 received
though the listener reaches "Waiting for messages"). The C++ zephyr↔zephyr pair
(`test_zephyr_cpp_talker_to_listener_e2e`) passes. So the C++ zephyr **subscriber**
also fails to receive from a native publisher — unlike the rust zephyr subscriber,
which DOES (rust `native_to_zephyr` delivered 41). The native↔Zephyr-C++ bridge is
broken in both directions; native↔Zephyr-rust only in the zephyr-pub direction.
Both were already in the #164 24-fail list (pre-existing, not the port change).

## Suspects / direction

- zenoh-pico publication declaration or the sample's key-expr / encoding not
  matching what the native `nros` subscriber (full zenoh) expects on the wire
  (a Zephyr↔Zephyr pair would both use the same pico encoding and hide it).
- The Zephyr publisher's session may not be completing its declare/flush to the
  router before the test window (though the Zephyr↔Zephyr pass argues the pub
  side does emit).
- Compare the on-wire publication (`zenohd` debug / a wireshark capture) from a
  Zephyr pico publisher vs a native publisher for the same topic.

Note: this was previously MASKED in the #164 re-triage — the rust zenoh cross
tests aborted on the #147 staleness guard before reaching delivery, so the
0-delivery only surfaced once slice 3's per-test routers + the
`NROS_SKIP_FIXTURE_CHECK` bypass let them run.

## References

`packages/testing/nros-tests/tests/zephyr.rs`
(`test_{zephyr_to_native,bidirectional_native_zephyr}_e2e`), issue #164 (the
family re-triage that surfaced it), phase-286 W1 (the parallelism work that
un-masked it).
