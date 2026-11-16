---
id: 2457
title: "cffi_pubsub_round_trip's session open failed with ConnectionFailed at ~10 s, once, in a merge-queue test-unit run, and printed no reason"
status: resolved
type: bug
area: [rmw, zenoh, testing, ci]
severity: low
found: 2026-11-12
resolved: 2026-11-17
related: [2338, 2389, 2207, 1976]
---

**Home: [phase-466](../../roadmap/phase-466-scheduled-lanes-produce-verdicts.md)**. A
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

## Item 1 done: the next sighting prints its reason and timings (2026-11-12)

- `nros_tests::log_sink::install_stderr_log_sink()` is an idempotent
  `nros_log` sink that writes each record to stderr, which nextest keeps for a
  failing test. `ZenohRouter`'s readiness wait (`wait_for_router_ready`, which
  every router constructor goes through) installs it, so every test that
  starts a router prints the open's own `[nros_log ERROR] nros_rmw_zenoh:
  zenoh open: …` line.
- The same wait prints `zenohd: accepting on <locator> N ms after spawn`.
- `cffi_smoke` opens through `open_client`, which prints `open <node> on
  <locator>: ok after N ms`, or puts the duration in its panic.

The sink was checked by a real failure: under plain `cargo test`, where the
binary's tests share one process, the second open now prints `zenoh session
pool exhausted — this build allows NROS_ZENOH_MAX_SESSIONS=1`. That line was
dropped before. Under nextest (one process per test, as CI runs it),
`cffi_smoke` and `zenoh_integration` pass 29/29.

Item 2 stands: either a second sighting names the phase, or the test stays
green through two weeks of merge-queue traffic (to 2026-11-26) and this
closes as not reproduced, with issue 2338.

## Second sighting names the phase: a stranger held the port (2026-11-15)

`just ci gate` on newslab-118 (main at 6ed248b898), step `test-unit`:

    nros-rmw-zenoh::zenoh_integration a_service_server_receives_through_the_ring_the_caller_supplied
    zenohd: accepting on tcp/127.0.0.1:34831 0 ms after spawn
    [nros_log ERROR] nros: zenoh open: no session with tcp/127.0.0.1:34831 in one attempt (connect_timeout_ms=0) (refused, unreachable, or no answer to the handshake)
    could not open a client session on tcp/127.0.0.1:34831: ConnectionFailed

The 0 ms is the evidence. A real `rmw_zenohd` on the same host took 15 to
20 ms from spawn to its first accept (6 starts, the probe's own Python start
included). So the first readiness connect reached a listener that was
already there before the router could exec, and the session opened onto it.

Whose: the host was shared with another tenant's ROS 2 / CARLA simulation at
the time, and every foreign loopback listener seen (six of them) was on an
odd ephemeral port, which is what `bind(0)` hands out on this kernel (2000 of
2000 odd; `connect()` source ports 2000 of 2000 even, so a TCP self-connect
is not the explanation). The port lease (`port_lease.rs`) makes a port
exclusive among OUR fixtures only. Its own doc says so ("it does not stop an
unrelated program from taking the port"), and then assumes "every colliding
party here is one of our own fixtures", which a shared host breaks.

This also fits the first sighting: a stranger that accepts TCP and never
answers zenoh's handshake gives exactly a 10 s `ConnectionFailed`.

### Fix: ready means the router's own process tree holds the listener

- `nros_tests::tcp_listener::holder(pid, port)` reads the LISTEN rows of
  `/proc/net/tcp{,6}` for the port's socket inode and asks whether `pid` or a
  descendant holds it (`fd -> socket:[inode]`): `Ours`, `Other`, or `Unknown`
  (no listener, or no `/proc`). Unit-tested both ways, with a negative
  control: the same listener seen from a child that does not hold it is
  `Other`.
- `ZenohRouter`'s readiness wait accepts only `Ours` (or `Unknown`, the old
  answer, where the host cannot say). A port that accepts with `Other` is
  printed once, and the wait ends `TestError::PortTaken` when the router
  exits or the wait times out.
- Every LEASED starter (`start_unique`, `start_unique_with_overrides`,
  `start_tls_unique`) goes through `on_fresh_lease`: a `PortTaken` takes a
  fresh lease, at most 3 in all, each said on stderr. This is not a retry of
  a failed test: the port was never ours, so a new one is the design. A
  starter handed a FIXED port reports `PortTaken` with the holder named.

Sweep, for every fixture readiness wait that is a bare TCP connect:

    git grep -n 'TcpStream::connect' -- packages/testing

The router fixture is the one with a sighting and a lease to refresh. The
generic `wait_for_port` / `wait_for_port_on` in `nros-tests/src/lib.rs` take
no pid and are used for QEMU's fixed forwarded ports in `emulator.rs`; they
stay as they are, and this records them as the same shape.

## Resolution (2026-11-17)

Fixed by 2d594b3445 (2026-11-15): a router is ready only when its own
process tree holds the listener, and a taken port gets a fresh lease. The
second sighting named the phase: a foreign listener on the leased port,
accepting 0 ms after spawn. No failure of `cffi_pubsub_round_trip`, or of
its sibling issue 2338's test, has been seen in a merge-group `test-unit`
since; the only merge-group failures after the fix were template-copy-out
stack-floor drift. One caveat stays on record: issue 2338's pre-fix
sighting accepted after 100 ms, not 0 ms, so that one is not proven to be a
stranger on the port.
