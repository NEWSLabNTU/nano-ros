---
id: 145
title: "Zephyr tx throughput hard-capped at ~1 send per socket recv window — Kconfig timeout is a band-aid, not a fix"
status: open
type: tech-debt
area: zephyr
related: [phase-276, phase-279, issue-0139]
---

> **In progress — [phase-279](../roadmap/phase-279-zephyr-tx-throughput-ceiling.md)**.
> W1 measured the ceiling (native_sim: 8.6 msg/s total at the 100 ms default —
> both a 100 Hz and a 10 Hz tier converge to ~4.3 msg/s each; 39 at 5 ms), and
> the W2 design is settled: an opt-in batch-mode flush in the SHARED zpico shim
> (`zp_batch_start` at open + guarded `zp_batch_flush` at the top of
> `zpico_spin_once`) — uniform across native/zephyr/freertos/nuttx/threadx/
> bare-metal by construction — with one `ZPICO_TX_BATCH` knob (six platform
> front-ends, default OFF) and a per-publisher `is_express` escape (native
> zenoh-pico bypass) for control tiers. Keepalives bound batch sit-time;
> batching state is tx-mutex-safe; overflow auto-flushes.

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
