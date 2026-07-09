---
id: 173
title: "Zephyr (native_sim) publisher → native subscriber delivers nothing through a shared zenoh router"
status: open
type: bug
area: rmw
related: [issue-0164, phase-286]
---

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
