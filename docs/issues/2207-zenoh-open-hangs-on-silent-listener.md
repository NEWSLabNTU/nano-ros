---
id: 2207
title: "`ZenohTransport::open` never returns when the locator's peer accepts the TCP connect and never answers the zenoh handshake"
status: open
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
