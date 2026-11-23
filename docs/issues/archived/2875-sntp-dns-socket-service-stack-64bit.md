---
id: 2875
title: "A 64-bit Zephyr image whose `clock.source` names `sntp` fails at configure: the SNTP source selects DNS, which pulls in the socket service thread with a stack below its floor"
status: resolved
type: bug
severity: medium
area: [clock, zephyr, build]
related: [rfc-0141, phase-582, issue-2785]
found: 2026-11-23
resolved: 2026-11-24
resolved_in: "zephyr: the module defaults the socket service stack to 4096 B on 64-bit, and the stack guard reads ZVFS_POLL_MAX first on Zephyr 4.x"
---

**Home: [phase-582](../../roadmap/phase-582-wall-clock-source.md)**, whose
SNTP source ([RFC-0141](../../design/0141-wall-clock-source.md)) adds the select
that pulls the thread in.

## What happens

On a 64-bit Zephyr target, an image with `clock.source` naming `sntp` stops at
configure. The guard in `zephyr/cmake/nros_socket_service_stack.cmake` (issue
2785) refuses its `CONFIG_NET_SOCKETS_SERVICE_STACK_SIZE`, and that stack
was never chosen by the image.

Seen on qemu-cortex-a53 with Zephyr 3.7 (`~/.nros/workspaces/zephyr/3.7`),
on an SNTP resync stack-walk probe image (2026-11-23). That image's own
`.config`:

```text
CONFIG_SNTP=y
CONFIG_NET_SOCKETS_SERVICE=y
CONFIG_NET_SOCKETS_SERVICE_STACK_SIZE=1200
CONFIG_NET_SOCKETS_POLL_MAX=3
# CONFIG_NET_DHCPV4 is not set
CONFIG_DNS_RESOLVER=y
CONFIG_DNS_RESOLVER_AUTO_INIT=y
```

The guard's floor on a 64-bit target is `POLL_MAX x 40 + 3072` = 3192 B,
against 1200. On a 32-bit target it is `POLL_MAX x 20 + 1024` = 1084 B, so
mps2-an385 passes at the same default and the bug shows only on 64-bit
boards.

## Why

- `zephyr/Kconfig`'s `config NROS_CLOCK_SOURCE_HAS_SNTP` (y exactly when
  `clock.source` names `sntp`) does `select SNTP` and `select DNS_RESOLVER`.
- In Zephyr 3.7, `config SNTP` only `depends on NET_SOCKETS`. It is
  `DNS_RESOLVER` (`subsys/net/lib/dns/Kconfig`) that selects
  `NET_SOCKETS_SERVICE`. With DHCPv4 off, the SNTP source's DNS select is
  the only route to the thread.
- Nothing raises `NET_SOCKETS_SERVICE_STACK_SIZE` to match, so Zephyr's
  default 1200 stands.
- A leaf's `CONFIG_DNS_RESOLVER=n` (the templates and the talker's
  mps2/nucleo/native_sim confs set it) cannot help. A `select` overrides it,
  so the line is dead once `sntp` is named.

The guard is right to refuse: on 64-bit, the poll array plus the DNS callback
frames overflow 1200 B silently (issue 2785). The bug is that choosing a clock
source hands the image a thread whose stack nobody sized.

## Wanted

Choosing `sntp` must not leave the image a socket service stack below the
guard's floor. Candidate shapes, not yet ruled:

1. The module sizes the stack when it selects the thread. The floor is
   already computed in cmake (`nros_socket_service_stack_floor`). The module
   states the stack the same way it states other derived Kconfig values,
   rather than leaving the image to raise it.
2. The SNTP source stops selecting `DNS_RESOLVER` when `sntp.server` is an
   IP literal, so a server given by address never pulls the thread in. A
   host name still needs DNS, so this narrows the bug without closing it.

The 32-bit case passes today only because its margin is smaller, not because
anything sized it. A fix should cover both widths.

## Acceptance

- A qemu-cortex-a53 image with `clock.source = "sntp"` and no stack line in
  its conf configures, builds, and passes the guard, on both Zephyr lines.
- The same image on mps2-an385 still configures.
- An image that does not name `sntp` gets no new socket service thread.

## Resolution

- `zephyr/Kconfig` states `configdefault NET_SOCKETS_SERVICE_STACK_SIZE` =
  4096 if `64BIT`, for every image that gets the thread. That covers the
  largest poll maximum the module defaults to (8, Cyclone: 3392 B). On 32-bit
  the floor at 8 polls is 1184 B, which Zephyr's 1200 already covers, so the
  32-bit default is unchanged. A board conf or prj.conf still wins, and the
  guard still refuses a stack below its floor, naming the size.
- The guard read the wrong poll knob on Zephyr 4.x. The array there is sized
  by `ZVFS_POLL_MAX`, but 4.x still defines the deprecated
  `NET_SOCKETS_POLL_MAX` with a default of 0, and the guard asked that first.
  It now asks `ZVFS_POLL_MAX` first.

Candidate 2 (no DNS for an IP-literal server) was not taken: it narrows the
bug without closing it.

### Measured

- qemu-cortex-a53 SMP, Zephyr 4.4, `clock.source = "sntp"`, no stack line:
  configures and builds. Its `.config` has `NET_SOCKETS_SERVICE=y`,
  `NET_SOCKETS_SERVICE_STACK_SIZE=4096`, `ZVFS_POLL_MAX=3`,
  `NET_SOCKETS_POLL_MAX=0`.
- Negative control, the same image with the stack set to 3000: the guard
  refuses it and asks for 3192 B, citing `CONFIG_ZVFS_POLL_MAX=3`. So the 4.x
  floor comes from `ZVFS_POLL_MAX`, not from the 0 the old order read.
- mps2-an385 C++ talker, Zephyr 4.4: configures and builds (it gets no socket
  service thread, so the guard does not apply).
- qemu-cortex-a53 SMP, Zephyr 3.7.2, `clock.source = "sntp"`, no stack
  line, fresh build dir, on main after the fix merged: configures and builds,
  with no refusal from the guard. Its `.config` has `NET_SOCKETS_SERVICE=y`,
  `NET_SOCKETS_SERVICE_STACK_SIZE=4096`, `NET_SOCKETS_POLL_MAX=3` (3.7 has no
  `ZVFS_POLL_MAX`, so this is the poll knob there): floor 3192 B, under 4096.
- ARM FVP, Zephyr 3.7: builds. FVP has no socket service thread.
