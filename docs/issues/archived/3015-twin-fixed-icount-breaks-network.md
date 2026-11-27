---
id: 3015
title: "The stack-floor twin's CONFIG_QEMU_ICOUNT fixes `-icount shift=4` on Zephyr's qemu_cortex_a53 line, and a networked run over slirp then loses its zenoh session"
status: resolved
type: bug
severity: low
area: [zephyr, stack-floor, testing]
related: [issue-2846, issue-3014]
found: 2026-11-28
resolved: 2026-11-28
resolved_in: "fix-3015-twin-icount-smp"
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

## Fixed (2026-11-28, host 139)

**No fixed shift works on this guest.** The same image was booted by hand
under each variant, with the router and the row's SNTP and DNS stand-ins:

| `-icount` | runs |
| --- | --- |
| `shift=4` (Zephyr's) | session lost, no SNTP answer |
| `shift=2`, `shift=6` | no SNTP answer in 40 s |
| `shift=0` | 42 answers, peak 2,176 B; the next run stalled at one round with no answer |
| `shift=1` | peaks 2,112 B and 2,256 B |

With two vCPUs and a network that runs on host time, icount bought neither
agreement nor a session that holds.

**The twin uses icount only on a single-CPU QEMU board.** `stack-floor-twin.py`
reads the board descriptor's `[board.run.qemu] smp = "cpus=N"`
(`twin_qemu_cpus`). It adds `CONFIG_QEMU_ICOUNT` only when that is 1, and an
SMP twin runs on Zephyr's own line without icount, as Zephyr does under
NETWORKING. Its row keeps the highest peak it has seen. The self-test checks
both boards, and a descriptor whose `smp` sits in the wrong table.

## Acceptance

- **Met:** the a53 row re-measures on the west road with no per-image
  `run.method`, which is dropped. Three twin runs gave peaks of 2,256, 2,256
  and 2,272 B, each with 228-229 SNTP answers. The row now records 2,272 B.
- **Met:** the 32-bit mps2-an385 row still runs under icount (one CPU) and
  agrees: peak 1,160 B with 151 SNTP answers. The twin confirms the walk
  covers the run.

