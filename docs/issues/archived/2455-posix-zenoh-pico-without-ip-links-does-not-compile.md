---
id: 2455
title: "zenoh-pico does not compile on POSIX with its IP links off, so a Linux image on a serial or custom locator cannot build"
status: resolved
type: bug
severity: low
area: [rmw-zenoh, build, linux]
related: [2452, 2389]
found: 2026-11-12
resolved: 2026-11-12
resolved_in: "zenoh-pico pin fb5f5cdc guards the POSIX TCP/UDP links on their features"
---

> **Resolved (2026-11-12).** The zenoh-pico pin moved to `fb5f5cdc`, which
> guards `tcp_posix.c` on TCP/TLS/WS and `udp_posix.c` on UDP unicast, the
> way their lwIP twins already were. `examples/linux/c/custom-transport-loopback`
> builds with its IP links out (no `NROS_LINK_IP` override) and its runtime
> test passes.

## Measured

A Linux image whose locator is on a non-IP link (`custom/…`, `serial/…`) gets
`NROS_LINK_IP=0` + `ZPICO_NO_SMOLTCP=1` from the link it states
(`leaf_settings::transport_implications_of`, `is_byte_stream_link`), so
zpico-sys builds zenoh-pico with `Z_FEATURE_LINK_TCP/UDP_* = 0`. On POSIX
that does not compile:

```
src/link/transport/tcp/tcp_posix.c:39:7: error: '_z_sys_net_endpoint_t' has no member named '_iptcp'
src/link/transport/udp/udp_posix.c:36:7: error: '_z_sys_net_endpoint_t' has no member named '_iptcp'
```

(issue 2452, building `examples/linux/c/custom-transport-loopback` once its
image stated `locator = "custom/loopback"`).

## Cause

`tcp_posix.c` and `udp_posix.c` are guarded on the socket family alone
(`ZP_PLATFORM_SOCKET_POSIX`), while `unix.h` declares
`_z_sys_net_endpoint_t._iptcp` only when an IP link is on. Their lwIP twins
already carry the link guard.

## Fix

Fork commit `fb5f5cdc` on `jerry73204/zenoh-pico` `nano-ros-1.10.1` ("the
POSIX TCP and UDP links compile only when enabled"); the pin,
`nros-sdk-index.toml`'s `[source.zenoh-pico] ref` and the forks page moved to
it in the same PR as issue 2452.
