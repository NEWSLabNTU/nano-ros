---
id: 2048
title: "NuttX board pushes `eth0`'s address with SIOCSIFADDR, overriding the kernel that owns it"
status: open
type: tech-debt
area: [boards, nuttx, network]
severity: low
found: 2026-10-25
related: [phase-514, RFC-0109]
---

## What

RFC-0109 D2 says NuttX owns its board's network (`[board.net] stack = "os"`)
and nano-ros only READS it. Phase-514 W4 moved the board's address default
there: `BOARD_NET_*` are now generated from `nuttx-config/<arch>/defconfig`'s
`CONFIG_NETINIT_*`, and the arm / arm-smp defconfigs were moved from
`10.0.2.30` to `10.0.2.15` so the generated value equals what the board
applied before (no runtime change).

But the board still WRITES the address at run time:
`nros-board-nuttx-qemu/src/node.rs::apply_ip_config` issues `SIOCSIFADDR` /
`SIOCSIFNETMASK` / `SIOCSIFDSTADDR` on `eth0` from `Config::ip()`, reached by
`entry_212n::configure_entry_eth0` (Rust and C entries alike) and by the
direct-mode presets (`Config::listener()` etc., `ip + 1/2/3`). Its comments
give two reasons: two guests sharing one network would collide (Phase
97.4), and "without this push the guest keeps its defconfig address
[`10.0.2.30`], never reaches slirp's `10.0.2.2`".

## Question to answer before retiring it

With the defconfig now stating `10.0.2.15`, does an image that skips the
push still reach the router? Every guest runs behind its own SLIRP, so the
collision reason applies only to a bridge/TAP network, where the address
would come from a NuttX config fragment the image names in `os_config`. The
`/dev/urandom` reseed from the IP (ZID uniqueness) is a separate concern and
needs its own source if the push goes.

## Done when

Either the push is gone and the NuttX QEMU cells pass with the kernel's own
address, or this issue records the measurement that shows it is still
needed and why.
