---
id: 191
title: "freertos rust *-entry e2e: boots + connects, 0 messages delivered (pubsub/service/action)"
status: open
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
