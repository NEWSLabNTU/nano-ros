---
id: 2779
title: "TCP end-of-stream is not reported as its own result on the Zephyr, FreeRTOS/lwIP, ThreadX and generic-POSIX platform ports: a client whose router dies never reopens there"
status: resolved
type: bug
area: [zenoh, platform, zephyr, freertos, threadx]
severity: high
found: 2026-11-21
related: [2774, 2389, 2366, 2799, 2798]
resolved: 2026-11-21
resolved_in: "fix(nros-smoltcp, issue 2779): a reopened TCP slot aborts its stale socket, so a bare-metal client reconnects"
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

## Fixed in the tree, 2026-11-21 (still open: the per-target measurement)

**The read each target compiles, checked against the manifests.** This
corrects "What is wrong" above:

| target | TCP read compiled | fixed by |
| --- | --- | --- |
| native Linux, NuttX | zenoh-pico `tcp_posix.c` | issue 2774 |
| Zephyr | zenoh-pico `tcp_zephyr.c` (`ZENOH_ZEPHYR` is defined; the alias TU's network section is elided, `zenoh-sources.txt` "Zephyr's vendored platform TUs") | fork `e9675d97` |
| FreeRTOS + lwIP | zenoh-pico `tcp_lwip.c` (the alias TU is off on FreeRTOS) | fork `e9675d97` |
| FreeRTOS on a host stack | zenoh-pico `tcp_posix.c` | issue 2774 |
| ThreadX (`NROS_PLATFORM_ALIASES`) | alias `_z_tcp_read` -> `nros_platform_tcp_read` (NetX Duo BSD) | the port |
| bare metal | alias -> `nros-smoltcp`'s `PlatformTcp::read` | the bridge |

So on Zephyr and FreeRTOS + lwIP, the nros port's `nros_platform_tcp_read`
is not what zenoh reads through. Their ports return the new result anyway,
because the ABI rule is that every port does.

What changed:

* `platform_net.h`: `NROS_PLATFORM_NET_SOCKET_EOF` (`SIZE_MAX - 1`), with
  Rust `nros_platform_api::NET_SOCKET_EOF`. The alias TU static-asserts that
  it equals `_Z_LINK_READ_EOF`.
* Every C port returns it when a `recv` yields 0 for a non-empty buffer, and
  its `read_exact` stops on it. ThreadX's "not readable" stays 0: that is
  decided before any `recv`.
* `nros-smoltcp` drains `CloseWait` and records the peer's close; `read`
  returns EOF once the staged bytes are read.
* zenoh-pico fork `e9675d97`: the Zephyr, lwIP, ESP32 and Windows links.
* ThreadX's unlinked `c/platform/threadx/network.c` is deleted.
* Gate `check-platform-tcp-eof`: every port, every Rust `PlatformTcp`, and
  every zenoh-pico TCP link that reads with BSD `recv()`.

Measured so far: the POSIX port's `c_port_posix_net` test (data, then EOF,
from `read` and `read_exact`), and a FreeRTOS fixture build that recompiles
`tcp_lwip.c`. The two-router re-open ("How to confirm each port") is not yet
run on any of these targets. The closing condition is unchanged.

## Measured per target, 2026-11-21

The two-router reopen, as a cell on each target:

1. Wait for one sample.
2. SIGKILL the router, so no zenoh CLOSE reaches the clients.
3. Keep it down 5 s, then restart it on the same port.
4. Wait up to 60 s for a sample.

The sequence is `nros_tests::router_outage`. The cells are
`zephyr::test_zephyr_listener_reopens_after_its_router_restarts`,
`rtos_e2e::test_rtos_listener_reopens_after_its_router_restarts` (FreeRTOS,
threadx-linux) and
`emulator::test_qemu_bsp_listener_reopens_after_its_router_restarts` (bare
metal). Each negative control rebuilt the image without that target's half of
the fix; its TCP read was checked in the binary with objdump.

| target | with the fix | without it | reopen after restart |
| --- | --- | --- | --- |
| Zephyr native_sim (`tcp_zephyr.c`) | pass (newslab 241) | fail, zenoh-pico at `e0b8077c` | 12.4 s cell |
| FreeRTOS QEMU (`tcp_lwip.c`) | 6 of 6 | 0 of 3, `tcp_lwip.c` at `e0b8077c` | 0.8 to 2.1 s |
| bare metal (`nros-smoltcp`) | 5 of 5 | 0 of 3, before the stale-socket fix below | 1.8 to 1.9 s |
| threadx-linux (`nros_platform_tcp_read`) | 10 of 10 with issue 2799 fixed (4 of 5 before) | 0 of 3, `net.c` before the fix | 1.7 to 1.8 s |

**Bare metal needed a second fix.** With the end-of-stream read in place,
zenoh-pico closed the session and reopened, but no SYN reached the router. The
router's `zenoh_transport=debug` log showed no client connect in the 60 s.
`SmoltcpBridge::tcp_close` freed the slot and left its smoltcp socket in
CloseWait. The reopen got the same slot, and the bridge dialled only from
Closed. It now aborts a socket still holding the slot's previous connection
before it dials.

**threadx-linux is not counted as passing.** In the red run the talker, not the
listener, exits with status 1 and no message at the outage. That is issue 2799,
a separate defect; it also happens in every unfixed run. This issue closes when
the threadx-linux cell passes 10 of 10 with 2799 fixed.

The `just` fixture path served stale ThreadX images after a C-port edit (issue
2798, fixed in #4525). Every row above was built either outside the rustc
wrapper or with that fix in place.

## Resolved, 2026-11-21

Every port in the table reports end-of-stream and reconnects, and each has a
cell that asserts it. Zephyr native_sim is among them, as "Closes when" asks.

threadx-linux was the last row open. With issue 2799's fix (a host `send()`
with `MSG_NOSIGNAL` in NSOS), the threadx-linux cell passed 10 of 10, each run
reopening 1.7 to 1.8 s after the restart.

Landed on main:

- the cells, `86e9160c06`;
- the bare-metal stale-socket fix, `f7dc31d3bb`;
- issue 2799's fix, `1c8b5075a4`;
- the zenoh-pico and platform-port reads, earlier in #4484.

