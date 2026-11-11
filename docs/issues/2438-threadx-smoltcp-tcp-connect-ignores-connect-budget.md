---
id: 2438
title: "On ThreadX (NetX BSD) and bare-metal smoltcp the zenoh TCP connect ignores `connect_timeout_ms`"
status: open
type: bug
severity: low
area: [rmw-zenoh, threadx, platform]
related: [2437, 2389]
found: 2026-11-11
---

The remainder of [issue 2437](archived/2437-zenoh-tcp-connect-unbounded-by-connect-budget.md).
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
