---
id: 3015
title: "The stack-floor twin's CONFIG_QEMU_ICOUNT fixes `-icount shift=4` on Zephyr's qemu_cortex_a53 line, and a networked run over slirp then loses its zenoh session"
status: open
type: bug
severity: low
area: [zephyr, stack-floor, testing]
related: [issue-2846, issue-3014]
found: 2026-11-28
---

## What happens

The stack-floor twin (`scripts/stack-floor-twin.py`) adds `CONFIG_QEMU_ICOUNT=y` to
every QEMU twin so its runs agree. On the west road Zephyr's run line then
carries `-icount shift=4,align=off,sleep=on`. On qemu_cortex_a53 SMP (2 CPUs)
over QEMU's user network, guest time runs ahead of the host's, and the image's
zenoh session does not hold. Measured on host 139, 2026-11-28, with the realtime-c
`smp_sntp_bringup:zephyr_smp_sntp_resync` image and a router on
`tcp/127.0.0.1:7447`:

| QEMU `-icount` | result |
| --- | --- |
| `shift=4,align=off,sleep=on` (Zephyr's line) | `zenoh open: no session ... in one attempt`: the 1 s one-attempt connect bound runs out in guest time |
| the same, with `connect_timeout_ms = 10000` | the open succeeds, then `a write to the router took no byte for 5000 ms`; SNTP and DNS never answered |
| `shift=auto` (the descriptor's QEMU line) | connects and re-syncs |

Zephyr itself defaults `CONFIG_QEMU_ICOUNT` to n under `NETWORKING`
(`default y if !NETWORKING`), so an ordinary networked image never meets this.
Only the twin turns it on.

## Workaround in tree

The image states `[image.<id>.run] method = "qemu"`, so `nros run` boots the
descriptor's QEMU line, which uses `-icount shift=auto`. Issue 2846's 64-bit
row was measured that way.

## Fix direction

Not decided. Options, for whoever takes it:
- The twin sets `CONFIG_QEMU_ICOUNT_SHIFT` to a value that keeps guest time
  behind the host's for networked QEMU twins. This needs a measurement per
  board.
- A networked QEMU twin runs on the descriptor's line (`shift=auto`) by
  default, giving up exact run-to-run agreement. The a53 runs read 2,192 B and
  2,112 B.
- The twin leaves icount off for networked images, as Zephyr does.

## Acceptance

- The a53 row re-measures on the west road without a per-image `run.method`.
- The 32-bit mps2-an385 row, which runs under icount today, still agrees run to
  run.
