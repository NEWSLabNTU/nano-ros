---
id: 145
title: "Zephyr tx throughput hard-capped at ~1 send per socket recv window — Kconfig timeout is a band-aid, not a fix"
status: resolved
type: tech-debt
area: zephyr
related: [phase-276, phase-279, phase-282, issue-0139, issue-0148]
---

> **Mitigated — [phase-279](../roadmap/archived/phase-279-zephyr-tx-throughput-ceiling.md)**
> (W1-W4). Opt-in `ZPICO_TX_BATCH` (env / Kconfig `CONFIG_NROS_ZENOH_TX_BATCH=y`)
> = tx batching + a dedicated flush thread (multi-threaded platforms except
> ThreadX; cadence `ZPICO_TX_BATCH_FLUSH_MS`, default 50 ms): **4× total
> throughput at the 100 ms default (34.1 vs 8.6 msg/s), 1.35× at 5 ms (52.5 vs
> 39), 10 Hz tier ≈ ideal at both.** Measured en route: flushing from the tier
> threads themselves is WORSE-or-equal (4.7-9.2) — the flush must own its own
> thread. Residual gap (100 Hz tier at 25-44 of 100): puts still block on the
> transport tx mutex while a flush-send is in flight, because zenoh-pico holds
> that mutex across the entire socket write. Remaining levers: fork surgery
> (release the tx mutex during the link write via a wbuf swap + link-write
> mutex), a second tx link, or the upstream zsock fd-lock release.
> **RESOLVED — [phase-282](../roadmap/phase-282-zenoh-tx-path-optimization-unified.md)
> (W1-W4, 2026-07-08).** Fork split-lock + overflow steal
> (`Z_FEATURE_TX_SPLIT_LOCK`, zenoh-pico fork ef065b9c): the flush STEALS the
> batch (wbuf swap) and writes the socket under a separate link mutex; an
> overflow PARKS the batch instead of blocking the publisher. Zephyr
> streaming measured **off 8.9 → batch ~136 → batch+split ~181 msg/s (20×)**
> with a tight-loop publisher completing 5000/5000 valid; tiers 43.2 total
> (5× baseline, telem ≈ ideal). Per-publisher `tx_express` QoS escape is
> uniform across Rust/C/C++; tuning decision tree in the book
> (user-guide/tx-tuning.md). Exit criteria met: streaming target far
> exceeded; the tier-target miss is EXPLAINED and re-scoped — the 100 Hz
> tier caps at ~40/s in every tx configuration, i.e. the residual is
> GENERATION-side (executor timer / native_sim scheduling), a different
> mechanism than this issue's zsock serialization → successor issue
> [[0148]]. Knobs stay default-OFF pending the promotion decision
> (options table in the phase doc §Promotion options); parked levers
> (second tx link, upstream zsock fd-lock release, hardware numbers) stay
> parked unless hardware demands them.

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
