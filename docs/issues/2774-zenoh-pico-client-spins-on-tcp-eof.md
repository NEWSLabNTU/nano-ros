---
id: 2774
title: "A zenoh-pico client whose router closes the TCP link without a zenoh CLOSE reads end-of-stream forever: it never reopens, burns a core, and a later session close can hang"
status: open
type: bug
area: [rmw-zenoh, zenoh-pico]
severity: high
found: 2026-11-21
related: [2366, 2596, 1749]
---

## What happens

Measured on host 118 with `constrained-link-probe` (a native nano-ros image, zenoh
client, threaded build) connected to a stock `rmw_zenohd`:

* The router is killed with SIGKILL. The client's socket goes to `CLOSE-WAIT`,
  and its zenoh-pico executor thread calls `recvfrom` on it about 1.5 million
  times in 30 s, each returning 0, with **one** `connect` in the whole run
  (`strace -f`). It never notices the link is gone, so it never reopens: a
  router started again on the same port is never dialled.
* The same happens when the test harness stops a router: the session is not
  reopened, though SIGTERM to a hand-started `rmw_zenohd` (which sends zenoh's
  CLOSE before exiting) is recovered from every time. The difference was not
  pinned down; what both failing cases share is a client that never saw CLOSE.
* The unfixed build behaves the same (`probe-control-rel`, main before issue
  2366), so this is not a regression.
* A session close on such a client can hang for good:
  `zpico_close` -> `z_session_drop` -> `_z_session_close` ->
  `_z_sync_group_wait`, waiting for a pending query's closure that is never
  dropped, because the query's timeout is a future on the executor that is
  spinning on the dead socket. Any outstanding `z_get` does it (a service call,
  or since issue 2366 a transient-local history re-query).

## Why, as far as read

`_z_tcp_posix_read` (`src/link/transport/tcp/tcp_posix.c`) returns `recv`'s 0
as "0 bytes read", the same value a non-blocking read with nothing waiting
gives, so end-of-stream is not distinguished from "no data yet". The datagram
path already special-cases 0 for a non-blocking link (`rx.c`,
`_z_link_recv_t_msg_cap_flow_datagram`); the stream read path has to tell a
real EOF apart and close the transport so `_z_client_reopen_task_fn` runs.

## Reproduce

Two routers, the publisher on A, the subscription on B (B connects to A); kill
B with SIGKILL after the subscription has read the first value, then start B
again on the same port. The subscription never reconnects. `strace -f -e
trace=recvfrom,connect` on the subscription shows the loop.

## Closes when

A client whose link reaches end-of-stream closes the transport and reopens
when the router is back, for SIGKILL and for the harness's stop, and a session
close with a query outstanding returns. The session re-open phase of
`interop_e2e::interop::case_13` (issue 2366: a value changed while the
subscription's session was away is read exactly once) is added with it, since
until then every router stop the harness can make measures this bug instead.
