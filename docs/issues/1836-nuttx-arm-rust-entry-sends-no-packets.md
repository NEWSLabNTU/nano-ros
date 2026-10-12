---
id: 1836
title: "Every NuttX arm Rust image fails its zenoh session open without putting a packet on the wire"
status: open
type: bug
area: [nuttx, rmw, testing]
severity: high
found: 2026-10-12
related: [0130, 1281, 1363]
---

## Measured

All three NuttX arm Rust cells fail on origin/main `96a880b1c0` (a clean
worktree, fixtures built by `just nuttx build-fixtures-arm`), the same way
as on the phase-484 W4b-2 branch:

```
[ERROR] nros: [    8.941000] zpico Session -> ConnectionFailed (the two are indistinguishable downstream)
[ERROR] nros: [    8.973000] RMW session open failed — ConnectionFailed
Executor::open failed: Transport(ConnectionFailed)
```

`test_rtos_{pubsub,service,action}_e2e::platform_2_Platform__Nuttx::lang_1_Lang__Rust`
— FAIL / FAIL / TIMEOUT. The C and C++ cells on the same board, kernel,
router and test pass (6/6).

The guest's own traffic, captured with QEMU's `filter-dump` on the guest
NIC (no host privileges; `tcpdump -r` reads it):

- **C `service-server`** (`tcp/10.0.2.2:8310`): ARP for 10.0.2.2 at once,
  SYN from 10.0.2.30, and the zenoh handshake completes.
- **Rust `service-server`** (`tcp/10.0.2.2:8210`, router listening on
  `0.0.0.0:8210`): **nothing**. The pcap holds only its 24-byte header — not
  an ARP, not a SYN — over 90 s of wall time, while the guest reports
  `ConnectionFailed` at 8.9 s guest time.

So the connect fails inside the guest before the stack ever resolves the
gateway: the failure is local (an unreachable route, `eth0` down, or the
socket call itself), not anything the router or SLIRP sees.

Ruled out: the address. The Rust entry pushing 10.0.2.15 (the QEMU network
default) or 10.0.2.30 (the arm kernel's `CONFIG_NETINIT_IPADDR`) fails the
same way, and so does a Rust `Config::default()` on the old 192.0.3.10
platform base.

## Next

The Rust path differs from C in `init_hardware` →
`apply_ip_config` (`packages/boards/nros-board-nuttx-qemu/src/node.rs`),
which pushes ip / netmask / `SIOCSIFDSTADDR` through raw ioctl numbers
(`_SIOC(N) = 0x0700 | N`) and ignores each return code. A wrong ioctl, or a
`SIOCSIFDSTADDR` the stack reads as something other than the default router,
leaves `eth0` with no route — which is exactly "fails before ARP". Reproduce
with `scratchpad`-style capture (router on the port, `-object
filter-dump,id=f0,netdev=net0,file=x.pcap`), then check the ioctl return
codes or break on `connect` under `gdb` against QEMU's `-s`.
