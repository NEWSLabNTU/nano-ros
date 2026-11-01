---
id: 2196
title: "`client_session_with_absent_locator_dials_backend_default` is red whenever anything on the host holds port 7447, including a zenoh router it could have used"
status: resolved
type: bug
severity: low
area: [zenoh, testing]
related: [0330, 1964, 1886]
found: 2026-10-31
---

## What

`nros-rmw-zenoh`'s `zenoh_integration` test
`client_session_with_absent_locator_dials_backend_default` (issue 0330's
regression test for the backend-owned default locator) has to use the real
default port, 7447. It probed the port first and failed as an unmet
precondition whenever the port was already bound. On a shared host that is
routine. It stopped `just ci gate` at `test-unit` in phase-520's gate run
(2026-10-31, the holder was another session's `just l3-router`, i.e.
`rmw_zenohd`), in issue 1964's run, and during issue 1886.

The usual holder is somebody's zenoh router, and the test's assertion holds
against any zenoh router: a client open with no locator anywhere completes a
zenoh handshake on the default port.

## Resolution (2026-11-01)

The test no longer treats a held port as unmet:

- **Port free:** unchanged. It starts its own router on 7447.
- **Port held:** it does not start or reap anything. It first opens a session
  with `DEFAULT_LOCATOR` given EXPLICITLY, in a thread bounded to 10 s. If that
  fails or never answers, the holder is not a usable zenoh router, and the test
  reports an unmet precondition naming it. If the explicit open works, the
  no-locator open must work too, and a failure there is a real failure of the
  default substitution.

The 10 s bound exists because a holder that accepts the TCP connect and never
answers the handshake blocks `ZenohTransport::open` indefinitely. Measured:
a bare Python listener on 7447 ran the test into nextest's 60 s timeout.

Measured on this host:

| 7447 held by | result |
| --- | --- |
| nothing | PASS |
| `rmw_zenohd` (via `nros_router_exec`) | PASS |
| bare TCP listener | `[UNMET PRECONDITION] … does not answer a zenoh handshake within 10 s` |
| `rmw_zenohd`, with `effective_client_locator`'s hosted default mutated to `None` | FAIL: `… must fall back to the backend default … got ConnectionFailed` |
