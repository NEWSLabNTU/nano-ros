---
id: 2457
title: "cffi_pubsub_round_trip's session open failed with ConnectionFailed at ~10 s, once, in a merge-queue test-unit run, and printed no reason"
status: open
type: bug
area: [rmw, zenoh, testing, ci]
severity: low
found: 2026-11-12
related: [2338, 2389, 2207, 1976]
---

**Home: [phase-466](../roadmap/phase-466-scheduled-lanes-produce-verdicts.md)**. A
merge-gating lane ejected a docs-only pull request (#3574) on it, so it costs a
queue batch each time it fires.

## Measured

Merge-queue `gate` run 43130480680 (2026-11-12 00:52 UTC), job 130893091044,
step `check / test-unit`:

    FAIL [  10.168s] (2447/2448) nros-rmw-zenoh::cffi_smoke cffi_pubsub_round_trip
    panicked at packages/rmw/zenoh/nros-rmw-zenoh/tests/cffi_smoke.rs:100:6:
    open: ConnectionFailed

The test leased a port, started its own `rmw_zenohd` through
`ZenohRouter::start_unique`, which returns once a TCP connect to that port
succeeds, and opened a client session on it. Nothing else was printed for it.

## What it is NOT

- **Not #3566's connect budget.** This image uses the builtin budget, which is 0:
  one attempt (`BUILTIN_ZENOH_CONNECT_TIMEOUT_MS`, issue 2207). With 0,
  zenoh-pico's `open_deadline` is NULL, so neither bound #3566 added applies:
  the TCP connect is a plain blocking `connect()` (`tcp_connect_within.h`, the
  `timeout_ms < 0` arm), and the handshake keeps zenoh-pico's own
  `Z_TRANSPORT_CONNECT_TIMEOUT` (10 s), both as before. The only #3566 code on
  the path is the stream write (`tcp_write_within.h`), which fails only after
  5 s with no byte taken. So 10.168 s reads as that 10 s handshake deadline
  plus router start: the router accepted the connect, and no handshake reply
  arrived in 10 s.
- **Not a test that opens before its router listens.** The fixture already
  waits for a TCP accept (`wait_for_router_ready`).
- **Not a port handed to a second test's router.** `drop(router)` releases a
  router's port lease, and the issue-1976 churn test restarts its router on
  the same port, which runs `kill_listeners_on_port` (`fuser -k`). If another
  test had leased that port in between, the restart would first wait up to
  `NAMED_PORT_WAIT` (240 s) for the lease. The churn test passed in 1.25 s.

## Not reproduced

On main (zenoh-pico at the pin, 2026-11-12), the test binary ran
`cffi_pubsub_round_trip` alone, each run starting its own router. Runs:
200 at 32 in parallel, then 400 at 64 in parallel with 32 busy loops. 600/600
passed, the slowest in 1.64 s.

No other `ConnectionFailed` appears in the eight failed merge-group runs from
2026-11-11 15:48 UTC on, before and after #3566 merged.

## A lead: issue 2338 is probably the same thing

[Issue 2338](2338-cffi-subscription-waker-flake.md) is a different test in the
SAME binary (`cffi_smoke`) that failed once at **10.187 s** (2410/2411). The
neighbour block printed beside it was also the issue-1976 churn test's
`z_declare_publisher failed` output. Here the neighbour was that same test
(`declares_keep_running_across_a_reconnect`, 1561 shim lines). Two sightings
five days apart share one binary, one ~10 s bound, a position at the very end of
the run, and one neighbour. That correlation is not yet a cause.

## The gap that makes this hard: the reason is dropped

The open's failure path does report why:
`zenoh open: no session with <locator> in one attempt (...)`
(`report_open_budget_spent`, nros-rmw-zenoh `zpico.rs`). It reports through
`nros_log`, which dispatches only to sinks registered with `nros_log::init`,
and a host integration test registers none. So CI printed only
`open: ConnectionFailed`, and the next sighting will be just as blind.

## What would close it

1. Make the next sighting say what happened. Register a stderr sink in the
   host tests that open sessions, so the open's own line reaches the log, and
   have `cffi_smoke` print how long the router took to accept and how long the
   open took.
2. With that, either a second sighting names the phase (connect, handshake,
   or a router that died), or the test stays green through two weeks of
   merge-queue traffic and this closes as not reproduced, together with 2338.

A stronger router readiness probe (a completed zenoh handshake before the
fixture returns) would cover every test that starts a router. It is held until
evidence shows the router answering late.
