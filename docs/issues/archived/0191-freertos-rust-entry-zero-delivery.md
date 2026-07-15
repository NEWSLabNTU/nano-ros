---
id: 191
title: "freertos rust *-entry e2e: boots + connects, 0 messages delivered (pubsub/service/action)"
status: resolved
resolved_in: "2026-07-15 — log-crate sink installed in nros-board-freertos entry (delivery worked all along; the marker lines were dropped)"
type: bug
area: freertos
related: [issue-0181, phase-287]
---

## Summary

With the #181 lane repairs (harness consumes the `*-entry` images, per-variant
ports 7451/7461/7471 baked, pair IPs split 10.0.2.15/.16, entry-banner
readiness gates), the freertos RUST lane now builds, boots, and OPENS its
zenoh session ("Application setup complete — entering spin loop" on the
correct per-variant port, default-slirp net plan) — but delivers nothing:

```
rtos_e2e test_rtos_{pubsub,service,action}_e2e::…Freertos::lang_1_Lang__Rust
  [freertos rust] messages received: 0
```

No `Publishing:` lines appear in the talker-entry output either — the entry
runtime's timer/publish path (or its log sink) never visibly fires.

## Context

- This lane was NEVER harness-green: pre-287 the entries baked
  `tcp/10.0.2.2:7447` while the harness listens per-(variant,lang) — first
  real exercise, exposed not regressed (same history as #179/#188).
- The C/C++ freertos lanes (same board, board-net plan) deliver green, and
  the freertos rust WORKSPACE entry (192.0.3 plan, `just freertos
  build-fixtures` ws lane) is exercised by other tests — compare its runtime
  wiring with the standalone `*-entry` images' `run_entry` path first.
- Follow-up candidate: converge the rust entries onto the board-net
  (192.0.3.x) plan the C/C++ images use, removing the per-lang launcher split
  in `rtos_e2e::start_process`.

## Resolution (2026-07-15) — missing `log` sink, delivery was fine

The images boot, open their session, register the node, publish, and
deliver — the harness just never saw it. The Node components
(`freertos_rs_talker` / `freertos_rs_listener`) emit their e2e markers
(`Publishing:` / `I heard:`) via `log::info!`, and `nros-board-freertos`
installed NO `log::Log` backend: every record was silently dropped, so the
marker-counting harness reported `messages received: 0` on a working lane.
(The threadx board grew `install_uart_logger` for exactly this; the nuttx
board logs via std stdout; freertos was the gap.)

Fix: ported the threadx `install_uart_logger` shape into
`nros-board-freertos/src/entry.rs` (fn-pointer indirection over
`BoardPrint`, installed in both the single-task entry and the tiers
entry, before user setup). All three lanes green on fresh fixtures:
`test_rtos_{pubsub,service,action}_e2e::…Freertos…Rust` 3/3
(35 s / 40 s / 42 s).

Red herrings ruled out on the way (recorded for #194, which is NOT this
class — the threadx board already has the logger): the `*-entry` pkgs'
`launch/system.launch.xml` files are empty step-2 placeholders with stale
"register() does nothing" comments, but bare `nros::main!()` never reads
the launch file — it is Form-1 self-bringup via the entry lib's
`pub use <node_pkg>::register` re-export, which is fully wired.
