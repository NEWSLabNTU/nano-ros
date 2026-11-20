---
id: 2779
title: "TCP end-of-stream is not reported as its own result on the Zephyr, FreeRTOS/lwIP, ThreadX and generic-POSIX platform ports: a client whose router dies never reopens there"
status: open
type: bug
area: [zenoh, platform, zephyr, freertos, threadx]
severity: high
found: 2026-11-21
related: [2774, 2389, 2366]
---

## What is wrong

Issue 2774 fixed the never-reopen for a zenoh client on zenoh-pico's own POSIX
TCP link (`tcp_posix.c`, the native Linux build). There, a read that reaches
end-of-stream now returns `_Z_LINK_READ_EOF`, the link records it as
`_rx_closed`, and the read task closes the session, which reconnects.

The embedded builds do not read TCP through that link. They are
`ZENOH_GENERIC` builds, so `_z_tcp_read` is nano-ros's alias in
`packages/rmw/zenoh/zpico-sys/c/zpico/platform_aliases.c`, which calls the
port's `nros_platform_tcp_read`. Every port passes `recv`'s 0 straight up:

| port | read call | on a peer close |
| --- | --- | --- |
| Zephyr (`nros-platform-zephyr/src/net.c`) | `zsock_recv` | returns 0 (BSD semantics), passed up as 0 |
| FreeRTOS + lwIP (`nros-platform-freertos/src/net.c`) | `lwip_recv` | returns 0 (BSD semantics), passed up as 0 |
| ThreadX + NetX Duo (`nros-platform-threadx/src/net.c`) | `nx_bsd_recv`, after `nros_threadx_tcp_readable` | 0 after a readable check; the port also returns 0 itself for "not readable", so the two must be told apart |
| generic POSIX (`nros-platform-posix/src/net.c`) | `recv` | returns 0, passed up as 0 |

`platform_net.h` documents only "bytes received, or
`NROS_PLATFORM_NET_SOCKET_ERROR`", so the ABI has no way to say end-of-stream,
and zenoh-pico reads the 0 as "nothing yet". The consequence is issue 2774's,
on every one of these targets: a client whose router dies without zenoh's
CLOSE polls the dead socket forever and never reopens, and a session close with
a query outstanding can hang. On a board this is an image that silently leaves
the graph for good after a router restart.

Which read a build compiles: the native Linux build defines `ZENOH_LINUX` and
reads through zenoh-pico's `tcp_posix.c` (fixed by 2774, measured). Zephyr
replaced zenoh-pico's system layer with the alias TU in phase 129
(`zpico-sys/zenoh-sources.txt`), and ThreadX links it too
(`nros-zpico-build/src/runner.rs`), so on those `_z_tcp_read` is
`nros_platform_tcp_read`. To be confirmed per target as part of the fix,
since a target that still compiled one of zenoh-pico's own TCP links
(`tcp_zephyr.c`, `tcp_lwip.c`, ...) would need the 2774 change there instead.
ThreadX also carries an older `_z_read_tcp` in
`zpico-sys/c/platform/threadx/network.c` that maps `rb <= 0` to `SIZE_MAX`;
whether anything still links it is part of the same check. `FreeRTOS_recv`
(FreeRTOS+TCP, not a nano-ros port) returns 0 on a timeout and a negative value
on a closed socket, so it would need no change.

## Fix, as planned

1. `platform_net.h`: a third result, `NROS_PLATFORM_NET_SOCKET_EOF`, for "the
   peer closed the stream", equal to zenoh-pico's `_Z_LINK_READ_EOF`
   (`SIZE_MAX - 1`), and `scripts/gen-abi-bindings.sh` for the bindings.
2. Each port returns it when its `recv` returns 0 for a non-empty buffer; on
   ThreadX only after the readable check, so "not readable" stays 0.
3. `platform_aliases.c` passes it through unchanged; `read_exact` stops on it.

## How to confirm each port

The 2774 measurement, per target: two routers, a transient-local subscription
on router B, B killed with SIGKILL and started again on the same port, the
publisher changing the value meanwhile. Before: no reconnect attempt after the
kill (the router's log shows no new client transport) and the new value never
read. After: a reconnect each second until B is back, and the value read
exactly once.

* Zephyr: `native_sim` with NSOS runs it on a host (host 241 can run it).
* FreeRTOS + lwIP and ThreadX: the QEMU / threadx-linux rows the rtos_e2e
  cells already use; ThreadX-linux needs the veth bridge.
* generic POSIX: any POSIX port image built `ZENOH_GENERIC`.

## Closes when

Each port above reports end-of-stream and reconnects in that measurement, with
a cell (or a `rtos_e2e` case) that asserts it on at least Zephyr native_sim.
