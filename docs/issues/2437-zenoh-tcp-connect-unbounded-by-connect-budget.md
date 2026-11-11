---
id: 2437
title: "A zenoh client's TCP connect is bounded by the stack, not by `connect_timeout_ms`, on native_sim, POSIX, NuttX and ThreadX"
status: open
type: bug
severity: low
area: [rmw-zenoh, zephyr, posix, nuttx, threadx]
related: [2389, 2207]
found: 2026-11-11
---

Split from [issue 2389](2389-zephyr-a53-open-silent-past-connect-budget.md),
whose fix gave the zenoh handshake the connect budget's deadline. The TCP
connect that comes before the handshake was swept on every platform and is
not bounded by the budget on four of them.

## Symptom (from reading, not measured)

`[knobs.zenoh.limits] connect_timeout_ms` bounds zenoh-pico's retry loop
(issue 2207) and, since issue 2389, each attempt's handshake read. It does
not reach the TCP `connect()` inside an attempt. Against a peer that
REFUSES the connect this does not matter: the refusal is immediate. Against
a peer that DROPS the SYN (an unreachable host, a firewall that drops), a
blocking connect waits for the stack's own SYN-retry budget, whatever the
image's budget is.

## Sweep (2026-11-11)

zenoh-pico's TCP open on each platform nano-ros builds:

| platform | TCP open | connect bounded by |
| --- | --- | --- |
| Zephyr native IP | `tcp_zephyr.c` | `CONFIG_NET_SOCKETS_CONNECT_TIMEOUT` (3000 ms in the a53 image) — bounded, but by Kconfig, not the budget |
| Zephyr native_sim | `tcp_zephyr.c` over NSOS | the HOST's blocking connect (Linux `tcp_syn_retries` = 6, about 127 s) |
| POSIX / NuttX | `tcp_posix.c` | the OS blocking connect (Linux about 127 s) |
| FreeRTOS lwIP | `tcp_lwip.c` | non-blocking connect + `select` with the socket timeout — bounded |
| ThreadX NetX BSD | `nros-platform-threadx/src/net.c` | `nx_bsd_connect` passes `NX_WAIT_FOREVER` (`nxd_bsd.c`), so only NetX's own SYN retries end it |
| bare-metal smoltcp | `nros-smoltcp` `platform_macro.rs` | the `timeout_ms` passed in, else `CONNECT_TIMEOUT_MS` (30000) — bounded |

So on native_sim, POSIX, NuttX and ThreadX a budget of, say, 3000 ms can
be overrun by about two minutes by one connect to a SYN-dropping peer.

Other transports: XRCE's session create (`uxr_create_session_retries`,
`XRCE_SESSION_CREATION_RETRIES = 3`, each waiting
`UXR_CONFIG_MIN_SESSION_CONNECTION_INTERVAL`) is bounded; Cyclone has no
connect step.

## Not known yet

- Measured overrun on each platform (nothing in the tree drops SYNs today; a
  test needs a non-routable address or a host firewall rule, which a
  container runner may not allow).
- Whether it should be fixed per platform (a non-blocking connect polled
  against the remaining budget, which zenoh-pico would have to pass to
  `_z_tcp_open`) or once, by handing the remaining budget to the link open
  the way issue 2389 handed it to the handshake.

## Acceptance

- On each platform above, an open against a SYN-dropping peer ends with
  `ConnectionFailed` and the budget line within `connect_timeout_ms` plus
  slack, or the platform's row says why it cannot.
