---
id: 145
title: "Zephyr tx throughput hard-capped at ~1 send per socket recv window — Kconfig timeout is a band-aid, not a fix"
status: open
type: tech-debt
area: zephyr
related: [phase-276, issue-0139]
---

## Summary

Zephyr's zsock layer serializes send/recv on a per-fd `fdtable` mutex held
for the entire blocking call, so the zenoh-pico read task owns the session
socket for a full `SO_RCVTIMEO` window between inbound packets, and every tx
(publish, declare, keepalive, query reply) queues behind it. Net effect:
**total image tx throughput ≈ 1 send per recv window** (plus inbound-traffic
wakes). Measured during 276-W2: at the 100 ms default, a 100 Hz + 10 Hz tier
pair throttled to ~5 msg/s each (~10 msg/s total).

The shipped mitigation is `CONFIG_NROS_ZENOH_SOCKET_TIMEOUT_MS` (default
100; the ws-realtime zephyr entry sets 5 ms → ~200 windows/s). That trades
read-task wakeup rate for tx budget and adds up to one window of tx LATENCY
per message — acceptable for native_sim demos, questionable for real boards
(power) and for anything faster than a few hundred Hz.

## Real-fix directions (pick after measuring on hardware)

1. **Second link**: open a dedicated tx socket (zenoh-pico multi-link or a
   second session for publishers) so tx never shares an fd with the blocking
   read. Cleanest semantics; needs zenoh-pico link plumbing.
2. **Batch mode**: zenoh-pico `Z_FEATURE_BATCHING` is compiled in but only
   active between explicit `zp_batch_start/stop` — a periodic flush driven by
   the executor spin could coalesce N puts per send window. Adds bounded
   batching latency; fits telemetry tiers, not control tiers.
3. **Upstream Zephyr**: a zsock/NSOS mode that releases the fd lock while
   parked in poll (the lock only needs to cover the actual syscall marshal).
   Biggest lever, hardest to land.

## References

platform-implementation-notes "Zephyr zsock per-fd serialization" section;
[[archived/0139]] (the lease-death variant of the same mechanism).
