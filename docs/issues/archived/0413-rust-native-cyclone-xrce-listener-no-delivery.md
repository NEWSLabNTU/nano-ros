---
id: 413
title: "Rust native cyclone/xrce example pair did not deliver — a STALE example binary"
status: resolved
type: bug
area: rmw
related: [phase-329, issue-0233, issue-0234]
resolved_in: da26485e9
---

## Resolution — it was a stale fixture, NOT a code bug

Surfaced by the phase-329 W4 native-example consumers: a same-language rust
cyclone/xrce talker+listener pair delivered nothing while C/C++ pairs did.

Root cause: the rust cyclone/xrce example binaries were **7 days stale** — never
rebuilt because no test had ever exercised those matrix cells — and the stale
binary panicked `Failed to open session: Transport(ConnectionFailed)` at
`Executor::open` (a session-OPEN failure, downstream of which the listener
printed nothing and read as "no delivery").

Wiping the example `target-cyclonedds` / `target-xrce` dirs and letting the
fixture harness rebuild fresh made every cell deliver:
- `native_example_pubsub_e2e` 9/9 green, `native_example_reqresp_e2e` all
  service+action cells green.

So the code was always correct — verified along the way: `register_type::<M>()`
runs on both the plain and `message_info` subscription paths; the Cyclone
descriptor registrar is installed by the board's `nros_rmw_cyclonedds_sys::
register()`; the CFFI `try_recv_raw_with_info` already falls back to plain take +
optional info; and both `Cargo.toml`s carry the `nros/rmw-cyclonedds` marker.
The carves in both consumers were dropped (`da26485e9`).

## Recurrence note

The staleness was the "untested cell rots" class: a matrix cell that no runtime
lane exercised sat un-rebuilt while its runtime dependency (the cyclonedds
install lib) moved underneath it. Now the W4 consumers run these cells in
`test-all`, so cargo keeps them current. A clean build was always green. If it
recurs on an incremental tree, it is the general fixture-freshness discipline
(rebuild after a `build/install/lib` cyclonedds re-provision), not a code fault.
