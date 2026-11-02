---
id: 2207
title: "`ZenohTransport::open` never returns when the locator's peer accepts the TCP connect and never answers the zenoh handshake"
status: resolved
type: bug
severity: medium
area: [zenoh, rmw]
related: [2196]
found: 2026-11-01
---

## What

Measured on a linux host (2026-11-01) while fixing issue 2196: a bare TCP
listener on `127.0.0.1:7447` (Python `socket.listen()`, which accepts and then
sends nothing) and a client-mode `ZenohTransport::open` with
`locator = tcp/127.0.0.1:7447`. The open never returned. The test ran into
nextest's 60 s timeout. With a real `rmw_zenohd` on the port, the same open
returns in milliseconds.

So a session open has no bound when the peer is reachable but not a zenoh
router: a wrong port, a half-up router, or any other service on the default
7447. Every caller blocks for good, with no error to report: a hosted
`nros::main!`, a C `nros_support_init`, and a test.

## Where to look (not yet confirmed)

- zenoh-pico 1.8.0's `_z_open` (`src/net/session.c`) reads
  `Z_CONFIG_CONNECT_TIMEOUT_KEY`, whose default is `"0"`, and loops over
  connect attempts against that budget. Whether `0` means "one attempt" or
  "no bound" here is not checked. The key sits behind `Z_FEATURE_UNSTABLE_API`.
- The handshake's read uses `Z_CONFIG_SOCKET_TIMEOUT` (5000 ms hosted), so a
  single read should time out. The hang suggests a retry loop around it, not
  one blocking read.
- `zpico.c` sets only `Z_CONFIG_CONNECT_KEY` for a client and passes no
  connect timeout.

## Direction

Give the client open a finite budget and surface its expiry as
`TransportError::ConnectionFailed` (or a timeout variant). If zenoh-pico
exposes the bound through `Z_CONFIG_CONNECT_TIMEOUT_KEY`, set it from a knob
in the registry (`knobs.toml`); otherwise fix the loop in the fork.
Acceptance: the bare-listener case above returns an error within the budget.
`client_session_with_absent_locator_dials_backend_default` currently works
around this by bounding its own probe (issue 2196).

## Resolution (2026-11-02)

The open was not unbounded; it took 103.7 s (measured), which nextest's 60 s
timeout cut short. The "where to look" guesses above were wrong:

- zenoh-pico bounds one attempt correctly. `SO_RCVTIMEO` is
  `Z_CONFIG_SOCKET_TIMEOUT` (5 s hosted), and `_z_unicast_handshake_open`
  gives up at `Z_TRANSPORT_CONNECT_TIMEOUT` (10 s).
- The multiplier was ours. `Context::connect_with_retry`
  (`nros-rmw-zenoh/src/zpico.rs`) retried every `zpico_open` failure 10 times
  with 300 ms backoff: 10 x 10 s + 9 x 0.3 s = 103.7 s. It had been sized for
  router-accept races in parallel test runs. The maintainer's rule (2026-11-02)
  is that a library retry follows from the library's goal, never from making
  a test pass, and that a test start race is fixed, not retried around.

Fix, by design rather than by a smaller budget:

- `connect_with_retry` is deleted. A session open is one `zpico_open` under a
  connect budget, and zenoh-pico applies its own policy within it: it retries
  only transport errors, sleeping 100 ms doubling to 1 s.
- The budget is two registry knobs resolved over the RFC-0049 ladder
  (builtin < platform < board < app):
  - `[knobs.zenoh.limits] connect_timeout_ms`, default `0` (one attempt);
  - `[knobs.zenoh.limits] connect_forever`, which outranks the budget.

  The value is resolved once (`resolve_zenoh_connect_timeout_ms`), emitted as
  `nros-rmw-zenoh`'s `CONNECT_TIMEOUT_MS`, and handed to zenoh-pico at open
  through `zpico_set_connect_timeout` (`Z_CONFIG_CONNECT_TIMEOUT_KEY`). Two
  knobs, not `-1`, because the ladder holds unsigned values only and Kconfig
  `-1` already means "not stated" for derivable knobs.
- zenoh-pico read that key only under `Z_FEATURE_UNSTABLE_API`, which no
  default build and no Zephyr C lane defines. The fork patch
  (`nano-ros-1.10.1`, "read the connect timeout without the unstable API")
  reads it in both branches.
- A board that boots before its router (AutoSDV's XIAO RP2350, router 30-90 s
  later) states `connect_forever = true` in its board toml.
- `test_rtos_session_open_outlasts_a_late_router` now runs its own listener
  row built with `NROS_ZENOH_CONNECT_FOREVER=1`, so it tests the knob. Every
  other cell runs the one-attempt default.

Measured on a linux host:
- `client_open_against_a_silent_listener_fails_within_a_bound` (bare,
  never-accepted listener, 30 s bound): 10.1 s with the fix; without it, the
  test hits the bound.
- With `NROS_ZENOH_CONNECT_FOREVER=1`, an open started with no router
  succeeded 0.5 s after a router came up 5 s later.
- Without the fork patch, the same build failed every open with
  `InvalidArgument` in 45 us.
