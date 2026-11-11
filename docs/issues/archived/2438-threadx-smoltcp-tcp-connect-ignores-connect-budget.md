---
id: 2438
title: "On ThreadX (NetX BSD) and bare-metal smoltcp the zenoh TCP connect ignores `connect_timeout_ms`"
status: resolved
type: bug
severity: low
area: [rmw-zenoh, threadx, platform]
related: [2437, 2389]
found: 2026-11-11
resolved: 2026-11-11
resolved_in: "nros_platform_tcp_open_within in every platform port, called by zpico/platform_aliases.c"
---

The remainder of [issue 2437](2437-zenoh-tcp-connect-unbounded-by-connect-budget.md).
2437 handed the connect budget to the TCP connect on every port that speaks
BSD sockets inside zenoh-pico (`tcp_posix.c`, `tcp_zephyr.c`). Two ports reach
TCP through nano-ros's platform ABI instead, `nros_platform_tcp_open(sock,
endpoint, timeout_ms)`, via `zpico-sys/c/zpico/platform_aliases.c`'s
`_z_tcp_open`. That ABI has no connect-budget argument, so zenoh-pico's
`_z_tcp_open_within` falls back to `_z_tcp_open` there and ignores the bound.

| port | connect today |
| --- | --- |
| ThreadX NetX BSD (`nros-platform-threadx/src/net.c`) | `nx_bsd_connect` on a blocking socket passes `NX_WAIT_FOREVER` (`nxd_bsd.c`): only NetX's own SYN retries end it |
| bare-metal smoltcp (`nros-smoltcp`, `platform_macro.rs`) | bounded, but by the `timeout_ms` passed in, else `CONNECT_TIMEOUT_MS` (30000), not the budget |

Not measured on either: no SYN-dropping run was made on threadx-linux or a
bare-metal QEMU board.

## Direction

- Add a connect bound to the platform ABI rather than a second spelling of
  the deadline: an `nros_platform_tcp_open_within(sock, endpoint, timeout_ms,
  connect_timeout_ms)` declared in `nros/platform_net.h` (the SSoT; regenerate
  `nros-platform-cffi` with `scripts/gen-abi-bindings.sh`), and the alias TU's
  `_z_tcp_open_within` calling it. Every port that implements
  `nros_platform_tcp_open` then implements it, or forwards to the old one.
- NetX BSD supports non-blocking connect: with
  `NX_BSD_SOCKET_ENABLE_OPTION_NON_BLOCKING` set, `nx_bsd_connect` passes a 0
  wait and returns `EINPROGRESS`, so the same connect-then-select shape as
  zenoh-pico's `tcp_connect_within.h` applies (`nx_bsd_fcntl` + `nx_bsd_select`).
- smoltcp: take the budget instead of `CONNECT_TIMEOUT_MS` when one is given.

## Acceptance

- threadx-linux: an open against a SYN-dropping peer ends with
  `ConnectionFailed` and the budget line within `connect_timeout_ms` plus
  slack. The same for a bare-metal smoltcp board, or its row says why not.

## Resolution (2026-11-11)

A new platform ABI symbol, declared in `nros/platform_net.h` (the SSoT,
RFC-0054) and regenerated into `nros-platform-cffi/src/generated.rs`:

```c
int8_t nros_platform_tcp_open_within(void *sock, const void *endpoint,
                                     uint32_t timeout_ms, int32_t connect_timeout_ms);
```

Negative `connect_timeout_ms` = no bound (exactly `nros_platform_tcp_open`).
`zpico/platform_aliases.c` defines `_z_tcp_open_within` over it with the
deadline issues 2389/2437 thread through the open, and
`zenoh_generic_platform.h` declares `Z_TCP_OPEN_WITHIN`, so zenoh-pico's own
fallback steps aside. No zenoh-pico change.

Every implementation of `nros_platform_tcp_open` gained it:

| port | how the connect is bounded |
| --- | --- |
| ThreadX (`nros-platform-threadx`) | `nx_bsd_fcntl` O_NONBLOCK (NetX's `NX_BSD_SOCKET_ENABLE_OPTION_NON_BLOCKING`: the connect waits 0 and returns `EINPROGRESS`), a write-select, SO_ERROR; the same calls run on the Linux host's nsos-netx shim |
| bare-metal smoltcp (`nros-smoltcp`, Rust `PlatformTcp::open_within`) | the SYN-ACK wait is the budget instead of `timeout_ms` / `CONNECT_TIMEOUT_MS` |
| POSIX C port | non-blocking connect + `poll` + SO_ERROR |
| Zephyr C port | the same over `zsock_*` |
| FreeRTOS lwIP C port | its existing non-blocking connect takes the budget instead of `timeout_ms` |
| cffi (`CffiPlatform`, `nros_platform_export_net!`) | dispatch and export of the new symbol |

`PlatformTcp::open_within` is required, not defaulted, so a future Rust stack
cannot silently ignore the bound.

### Measured

Peer `tcp/192.0.2.1:7447` (TEST-NET-1, SYN dropped), budget 3000 ms. "Before"
is the same build with the alias's `_z_tcp_open_within` forwarding to
`nros_platform_tcp_open` (the pre-fix behaviour), built side by side:

| port | before | after |
| --- | --- | --- |
| threadx-linux (NetX BSD API over nsos-netx) | **135.8 s** to the budget line | **3.02 s** |
| bare-metal smoltcp, mps2-an385 QEMU (slirp; locator edited to 192.0.2.1 for the run) | 3.10 s | 3.00 s |
| POSIX C port, `nros_platform_tcp_open_within` direct (unit test, 300 ms bound) | — | 0.30 s |

bare-metal was already inside the budget: the alias passes zenoh-pico's
socket timeout (100 ms) as `timeout_ms`, which smoltcp used as its connect
bound, and zenoh-pico retried until the budget. Now one attempt may wait up to
the remaining budget for a slow SYN-ACK instead of 100 ms.

Not run: ThreadX on real NetX Duo (threadx-qemu-riscv64), the Zephyr C port on
a native-IP board, the FreeRTOS C port. Each compiles the same shape; the
Zephyr port compiled in the native_sim image above.

### Not covered by a runtime cell

The threadx-linux and bare-metal measurements were taken by hand. The ABI
function itself is held by two POSIX C-port tests
(`tests/c_port_posix_net.rs`: a bounded open still connects to a listener; a
SYN-dropping peer ends at a 300 ms bound). A threadx-linux cell for the
SYN-drop case would need its own fixture row built with
`NROS_ZENOH_CONNECT_TIMEOUT_MS`, like the 2207 `connect_forever` row.
