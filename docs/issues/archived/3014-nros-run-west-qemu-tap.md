---
id: 3014
title: "`nros run` of a qemu-cortex-a53 Zephyr image boots Zephyr's TAP line (`ifname=zeth`), which an unprivileged host cannot open, while the board states a user-mode network"
status: resolved
type: bug
severity: medium
area: [zephyr, cli, boards]
related: [issue-2846, issue-3015]
found: 2026-11-28
resolved: 2026-11-28
resolved_in: "fix-3014-qemu-user-net"
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

## Fixed (2026-11-28, host 139)

- **The board takes the user network.** `nros-board-qemu-cortex-a53/prj.conf`
  states `CONFIG_NET_QEMU_USER=y`, so Zephyr's own run line is
  `-nic user,model=e1000`. The same file documents the TAP opt-in: an image
  `os_config` fragment with `CONFIG_NET_QEMU_ETHERNET=y` and its own
  addresses.
- **The gate covers every west-road QEMU board.**
  `just check west-qemu-user-net` (`scripts/check-west-qemu-user-net.py`):
  a board package with a `west_board`, a `qemu:` emulator and
  `CONFIG_NETWORKING=y` must state `CONFIG_NET_QEMU_USER=y` or an explicit
  `CONFIG_NET_QEMU_ETHERNET=y`. Today it finds 2 boards, mps2-an385-zephyr and
  qemu-cortex-a53, and both pass. Negative control: with the new line removed
  from the a53 board, it refuses that board. Its self-test covers the
  out-of-scope boards (Renode, off the west road, no networking) and a
  `[[board]]` list descriptor.

## Acceptance

- **Met:** a scaffolded image boots unprivileged. `nros new a53ws --template
  talker --lang c --board qemu-cortex-a53`, then `nros run a53ws_node
  --expect "Publishing:"`, matched `Publishing: 'Hello World: 1'` after
  4.1 s, on Zephyr's own line (`-nic user,model=e1000`), with the router
  `nros run` started on `tcp/127.0.0.1:7447`. Before the fix, the same road
  failed at `run_qemu` on the TAP line.
- **Met:** the gate and its negative control, above.
- **Not met, by a different cause:** issue 2846's image keeps
  `[run] method = "qemu"`. The TAP is gone, but the stack-floor twin turns
  `CONFIG_QEMU_ICOUNT` on, and Zephyr's line then fixes `-icount shift=4`,
  under which this 2-CPU guest loses its session over slirp. That is
  issue 3015's, and the image's comment now says so.


**Update (2026-11-28):** issue 3015 is fixed. An SMP twin now runs without icount, so the 2846 image dropped its `[run] method = "qemu"`, and every acceptance item here is met.
