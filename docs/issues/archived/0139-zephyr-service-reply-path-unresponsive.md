---
id: 139
title: "Zephyr native_sim service/queryable reply path unresponsive — `ros2 lifecycle get` (and `ros2 service list`) never answered by the Rust zephyr entry"
status: resolved
type: bug
area: zephyr
related: [phase-276, phase-264]
---

## Summary

The phase-276 W3 lifecycle-on-Zephyr fixture (`ws-lifecycle-rust/src/zephyr_entry`, built
on the #128 `apply_lifecycle` emit + the #129 lane fixes) registers the five REP-2002
lifecycle services and IS discoverable — `ros2 lifecycle nodes` lists `/zephyr_entry` —
but the **get-state service never answers**: `ros2 lifecycle get --no-daemon` (spin-time
0.1 → 2 → 5 s, polled for 30–40 s) returns empty / "Node not found", and a manual probe
shows `ros2 service list` empty against the running image. Pub/sub on the SAME image
lane is proven (the params + base entries deliver cross-process, #129), so this isolates
the **service/queryable reply path** on Zephyr native_sim.

## Root cause (2026-07-03)

Not a queryable/reply defect at all — the whole session was starving and silently
dying. Two stacked causes, both in the zenoh-pico Zephyr socket layer:

1. **Zephyr `zsock` serializes send/recv on a per-fd mutex** (`fdtable` entry lock,
   held for the entire blocking op — NSOS offload included). The zenoh-pico read
   task's blocking `recv` therefore holds the socket for a full `SO_RCVTIMEO`
   window between inbound packets, and EVERY tx (entity/liveliness declare, lease
   keepalive, publish, query reply) queues behind it.
2. **`Z_CONFIG_SOCKET_TIMEOUT` was 5000 ms on Zephyr** (vendored
   `zenoh-pico/include/zenoh-pico/config.h` lumped `ZENOH_ZEPHYR` with
   `ZENOH_NUTTX`; May-15 commit, no zephyr-specific rationale). With a 5 s recv
   hold per cycle, each tx waited up to 5 s for the mutex, boot took ~30 s
   (12 serialized declares), and — fatally — the client's lease keepalives
   (every ~3.3 s) missed the 10 s lease window, so **zenohd silently dropped the
   session**. The image kept spinning against a dead transport: discovery data
   sent before death stayed cached (nodes listed), every later query went
   unanswered. gdb: main thread parked in `z_impl_zsock_sendto → k_mutex_lock`,
   fd mutex owned by `_zp_unicast_read_task` inside `nsos_wait_for_poll`
   (`recv_timeout = 5000 ticks`).

Same mechanism family as #129 (the per-node-liveliness declare "deadlock" was the
first tx to hit the window) and the phase-276 W5 QoS-entry wedge.

## Fix

Fork patch (jerry73204/zenoh-pico, `include/zenoh-pico/config.h`): drop
`ZENOH_ZEPHYR` from the 5000 ms branch (NuttX keeps it), add an
`#ifndef Z_CONFIG_SOCKET_TIMEOUT` guard, so Zephyr gets the 100 ms the
unix/freertos ports use — the read task releases the fd mutex ≥10×/s, tx latency
drops from ≤5 s to ≤100 ms, keepalives stay inside the lease. Mirrored in
`zpico-sys/c/platform/zenoh_generic_config.h` (the ZENOH_GENERIC twin).

Result: entry boot 29 s → ~3 s; `ros2 lifecycle get /zephyr_entry` → `active [3]`;
all five REP-2002 services listed AND answering; `lifecycle_zephyr_entry_e2e`
un-ignored and green (9.4 s). The phase-264 param-services half rides the same fix.
