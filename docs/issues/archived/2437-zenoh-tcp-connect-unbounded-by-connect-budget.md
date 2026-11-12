---
id: 2437
title: "A zenoh client's TCP connect is bounded by the stack, not by `connect_timeout_ms`, on native_sim, POSIX, NuttX and ThreadX"
status: resolved
type: bug
severity: low
area: [rmw-zenoh, zephyr, posix, nuttx, threadx]
related: [2389, 2207, 2438]
found: 2026-11-11
resolved: 2026-11-11
resolved_in: "zenoh-pico fork 6827fe1f (the budget bounds the TCP connect on the BSD-socket ports); ThreadX and bare-metal smoltcp remain as issue 2438"
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

## Resolution (2026-11-11)

Measured on this host (Linux, default route present, `tcp_syn_retries` = 6)
against `tcp/192.0.2.1:7447` (TEST-NET-1: routed nowhere, so the SYN is
dropped and nothing answers), budget `connect_timeout_ms = 3000`:

| platform | before | after |
| --- | --- | --- |
| a plain blocking `connect()` (Python, for reference) | 132.4 s, `ETIMEDOUT` | — |
| Linux (`ZenohTransport::open`, `tcp_posix.c`) | **133.1 s** to `ConnectionFailed` | **3.00 s** |
| Zephyr native_sim (NSOS host sockets, `tcp_zephyr.c`) | **136.2 s** to the budget line | **5.0 s** from the spawn (2 s network wait + 3.0 s) |

### Fix

zenoh-pico fork, `nano-ros-1.10.1`, commit `6827fe1f` (on top of 2389's
`f84c6be5`). The deadline 2389 introduced is extended to the link open, not
spelled a second time: `_z_open_link(..., open_deadline)` parks it on
`_z_link_t` for the duration of `_open_f`, and `_z_f_link_open_tcp` turns it
into the remaining milliseconds for `_z_tcp_open_within`.

The BSD-socket ports share ONE helper, `src/link/transport/tcp/tcp_connect_within.h`:
a non-blocking `connect`, `poll` for `POLLOUT` bounded by the remaining
budget, `SO_ERROR` for the outcome, and the socket's flags restored.
`tcp_posix.c` (Linux, macOS, BSD, NuttX) and `tcp_zephyr.c` (Zephyr's own
stack and native_sim's NSOS) include it. A budget of `0` (one attempt) or `-1`
(`connect_forever`) passes no deadline and keeps the blocking connect.

### Per platform

| platform | state |
| --- | --- |
| Linux / POSIX | fixed, measured above |
| Zephyr native_sim | fixed, measured above |
| Zephyr native IP (the a53, mps2) | fixed by the same `tcp_zephyr.c` code; compiled, not run (was already bounded by `CONFIG_NET_SOCKETS_CONNECT_TIMEOUT`, 3000 ms) |
| NuttX | fixed by the same `tcp_posix.c` code; not run (no NuttX fixture built here) |
| FreeRTOS lwIP | unchanged: already bounded by its own non-blocking connect |
| ThreadX NetX BSD, bare-metal smoltcp | NOT fixed: they reach TCP through nano-ros's `nros_platform_tcp_open`, whose ABI has no connect bound. [Issue 2438](2438-threadx-smoltcp-tcp-connect-ignores-connect-budget.md), since resolved |

### Regression coverage

`tests/zephyr_connect_budget.rs` gained
`zephyr_zenoh_open_against_a_syn_dropping_peer_ends_at_its_budget` (same
cell, same image, locator `tcp/192.0.2.1:7447`). After: passes, 5.16 s. With
the fixture rebuilt on 2389's pin `f84c6be5`: fails at the test's 30 s wait
with no budget line. A host with no default route answers `ENETUNREACH` at
once; the open then retries that until the budget, so the case still holds
there but stops exercising the connect's own bound (said in the test).

### 2389's send-timeout suspect

Not this code path. The connect is `_z_tcp_open*`; a send after a successful
open is `_z_tcp_write` (`send` with no `SO_SNDTIMEO` on Zephyr), which this fix
does not touch.
