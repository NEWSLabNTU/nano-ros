---
id: 3014
title: "`nros run` of a qemu-cortex-a53 Zephyr image boots Zephyr's TAP line (`ifname=zeth`), which an unprivileged host cannot open, while the board states a user-mode network"
status: open
type: bug
severity: medium
area: [zephyr, cli, boards]
related: [issue-2846]
found: 2026-11-28
---

## What happens

`nros run` of any `qemu-cortex-a53` Zephyr image fails before the guest
prints a line. The board's `[board.run] emulator = "qemu:virt"` on the west
road picks the `west` runner, which boots Zephyr's own `west build -t run`
line:

```text
qemu-system-aarch64 -cpu cortex-a53 -nographic -machine virt,gic-version=3
  -nic tap,model=e1000,script=no,downscript=no,ifname=zeth ...
FAILED: [code=1] zephyr/CMakeFiles/run_qemu
```

A TAP needs `/dev/net/tun` and `CAP_NET_ADMIN` (`zeth` is made by Zephyr's
net-tools as root), so an unprivileged host cannot open it.

The board's own `prj.conf` states QEMU's user-mode network: guest
`10.0.2.15`, gateway (the host) `10.0.2.2`, and locator `tcp/10.0.2.2:7447`.
Over a TAP those addresses are wrong too: the host is whatever `zeth` was
given. So even with root the image dials nothing that answers.

Found by host 139 on 2026-11-28, running the stack-floor twin for issue
2846. The test harness never saw it: `nros_tests::zephyr::west_qemu_boot`
boots the descriptor's line with the NIC on the user network, deliberately
(its doc says so), and does not go through `nros run`.

## Why

Zephyr's `kernel.cmake` emits `-nic user,model=...` only when the Kconfig
choice is `CONFIG_NET_QEMU_USER`. The qemu_cortex_a53 board picks
`NET_QEMU_ETHERNET` under `NETWORKING` by default, which is the TAP line.
`nros-board-mps2-an385-zephyr/prj.conf` sets `CONFIG_NET_QEMU_USER=y`;
`nros-board-qemu-cortex-a53/prj.conf` does not. Today these are the only two
Zephyr boards on the west road with a QEMU emulator. Nucleo-F767ZI runs on
Renode.

## Fix direction

- **User-mode network by default.** The board package states
  `CONFIG_NET_QEMU_USER=y`, as mps2-an385's does, so Zephyr's own run line
  attaches the e1000 to QEMU's user network. The NIC model is unchanged, so
  `west_run_disagreements` still agrees.
- **TAP is an opt-in**, for a user who has a bridge: an image fragment that
  selects `CONFIG_NET_QEMU_ETHERNET` and states its own addresses.
- **Close the class with a gate:** a board on the west road with a `qemu:`
  emulator and `NETWORKING` must state `NET_QEMU_USER` (or an explicit TAP
  opt-in), so the next QEMU board cannot ship the TAP line silently.

## Workaround in tree

Issue 2846's measured image, realtime-c `smp_sntp_bringup:zephyr_smp_sntp_resync`,
states `[image.<id>.run] method = "qemu"`. That routes `nros run` to the
descriptor's QEMU runner, whose plan has `nic = "user,model=e1000"`. It fixes
one image, not the class.

## Acceptance

- `nros run` of a scaffolded `nros new <ws> --board qemu-cortex-a53` image
  boots and reaches its router on `10.0.2.2` unprivileged.
- The gate refuses a west-road QEMU board with networking and no
  `NET_QEMU_USER`, with a negative control.
- The 2846 image's `run.method` workaround is dropped.
