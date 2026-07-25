---
id: 269
title: "freertos/zenoh-pico: SubscriberCreationFailed once aggregate entity count crosses a threshold — every component passes alone, the union fails"
status: open
type: bug
severity: high
area: zpico
related: [issue-0255]
---

## Finding (autoware_sentinel phase-14 pin bump, 2026-07-25)

The sentinel's monolithic node (comp-all: 37 pubs / 21 services / 5 subs /
1 timer on ONE zenoh-pico session, MPS2-AN385 QEMU, lwIP + LAN9118, zenohd
over SLIRP) fails at wiring time with `Transport(SubscriberCreationFailed)`.
Bisection with the sentinel's per-component feature gates:

- every component feature ALONE boots and spins;
- the 4-combo (mrm + cmd-gate-extra + validator + op-mode-mgr,
  ~33 pubs / 19 svcs / 3 subs) boots and spins;
- adding comp-engagement (+4 pubs / +2 subs / +2 svcs) tips it into
  `SubscriberCreationFailed`.

Ruled out (reproduced with all of): `ZPICO_MAX_SUBSCRIBERS=64`,
`ZPICO_MAX_LIVELINESS=160`, `ZPICO_MAX_LARGE_SUBSCRIBERS=8`,
`ZPICO_MAX_PUBLISHERS=56`, `ZPICO_MAX_QUERYABLES=32`, 2.5 MiB FreeRTOS
heap, 768 KiB app-task stack. Subscriptions use the default 1024 B rx
hint (small payload class; 5 subs ≪ 64 slots), so neither shim pool cap
in `shim/subscriber.rs` should trip — suspicion falls on the
`declare_subscriber_ring_raw` error path (`TransportError::from(e)`)
after ~50+ prior declares on the session.

**The identical comp-all topology boots and spins on
nros-board-nuttx-qemu-arm** (std platform, BSD sockets) on the same pin
(`21a3a4248`) — the wall is specific to the freertos+lwip platform layer.

This looks like the same class as the sentinel's Phase-13.K1
"declare-storm" hang (the reason those bisection gates exist) — now a
hard error instead of a hang.

## Repro

autoware_sentinel branch `phase-14`, `src/autoware_sentinel_freertos`:

```sh
cargo build --release --features comp-all
zenohd --listen tcp/0.0.0.0:7447 &
qemu-system-arm -cpu cortex-m3 -machine mps2-an385 -nographic \
  -semihosting-config enable=on,target=native \
  -kernel target/thumbv7m-none-eabi/release/autoware_sentinel_freertos
# → "Application error: Transport(SubscriberCreationFailed)"
# 4-combo control: --features comp-mrm,comp-cmd-gate-extra,comp-validator,comp-op-mode-mgr → spins
```

## Ask

Diagnosis needs visibility: `ZENOH_DEBUG` is hardcoded `0` in
nros-zpico-build's config (runner.rs) and the rmw shim's `log::debug!`
lines are `feature = "std"`-gated — there is no way for an embedded
consumer to see WHICH declare fails or why. Either surface the zenoh-pico
error code in `TransportError`, or make the debug define overridable.
