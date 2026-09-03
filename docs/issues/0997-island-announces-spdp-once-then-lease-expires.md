---
id: 997
title: "A FreeRTOS Cyclone participant stops announcing seconds after the first real traffic arrives, so every peer expires its lease and deletes it"
status: open
area: [rmw, platform, embedded]
severity: high
related: [0917, 0888, phase-177]
---

# The announcements are correct, on schedule, and then they stop

## What was measured

Autoware Safety Island on QEMU `mps3-an536` (FreeRTOS, Cortex-R52, nano-ros
Cyclone backend), against a ROS 2 Humble publisher on the host over stock
`rmw_cyclonedds`, on a paced tap. Cyclone discovery tracing on the HOST side:

```
1788405861.745  SPDP ST0 1108270:34688031:ce03eb9a:1c1 bes fc3f NEW
                  (nano-ros/0.10.5/Linux/Generic) data udp/192.0.3.10:58376
1788405942.943  gc: lease expired: l 0x… guid 1108270:34688031:ce03eb9a:1c1
                  tend 1883239996692042 < now 1883239996755175
1788405942.943  gc: ddsi_delete_proxy_participant_by_guid(…:1c1) - deleting
1788405942.943  gc: delete_ppt(…:1c1) - deleting endpoints
```

Then every proxy reader and writer belonging to that participant is
garbage-collected, and the publisher's own match count goes `1` → `0` three
seconds later:

```
1788405861  matched kinematic_state=1 acceleration=1 steering_status=1
1788405946  matched kinematic_state=0 acceleration=0 steering_status=0
```

**Announcements from the island in the whole run: one.**

```
$ grep -c "SPDP ST0 1108270:34688031:ce03eb9a:1c1" cyclone-discovery.log
1
$ grep -c "SPDP" cyclone-discovery.log
2
```

## Why one is wrong

The embedded config in `nros-rmw-cyclonedds/src/session.cpp`
(`kEmbeddedCycloneConfig`) sets no `Discovery/SPDPInterval` and no
`Discovery/LeaseDuration`, so Cyclone's defaults apply
(`third-party/dds/cyclonedds/src/core/ddsi/defconfig.c`):

```c
cfg->spdp_interval  = INT64_C (30000000000);   /* 30 s */
cfg->lease_duration = INT64_C (10000000000);   /* 10 s */
```

and `handle_xevk_spdp` schedules the resend
(`src/core/ddsi/src/q_xevent.c:1044-1056`):

```c
/* schedule next when 80% of the interval has elapsed, or 2s
   before the lease ends, whichever comes first … */
else if (ldur < DDS_SECS (10)) intv = 4 * ldur / 5;
else                           intv = ldur - DDS_SECS (2);
if (intv > gv->config.spdp_interval) intv = gv->config.spdp_interval;
```

With `ldur = 10 s` that is `intv = 8 s`, capped by the 30 s interval — so the
island should announce roughly every **8 seconds**, about ten times across the
81-second window. It announced once.

The `tev` thread that drives those timed events is not missing: the same config
names it and gives it a 16 KiB stack, precisely because the FreeRTOS default of
1 KiB is too small for any Cyclone worker (phase 177.26). So this is not "the
thread was never created" — that would have failed differently and earlier.

## What it costs

Every DDS peer deletes the island roughly a minute or two into a run, and it
never comes back. On the receive side that is total: after the deletion the
island's readers no longer exist for the publisher, and the application sees
nothing arrive again, forever, while its control loop keeps spinning and
correctly reporting that its inputs are stale.

This is a long-run defect, so a short test does not see it. The an536 delivery
sweep (`autoware-safety-island/scripts/an536-size-sweep.sh`) walks seven
trajectory sizes; a run that finishes inside the window reports every size
delivered, and a run that crosses it reports every size failing — including
908 B, which is a single fragment and involves no fragmentation at all. That
non-determinism is what sent an entire day's measurements chasing a payload-size
cliff that was not there. Worth stating plainly for issue
[0917](0917-an536-fragmented-sample-never-syncs.md), whose recorded
size-vs-rate curve was presumably taken inside the window: a sweep of this shape
cannot distinguish "too big" from "the participant is gone", and its numbers
should be re-taken once this is fixed.

## AMENDED 2026-09-03 — island-side tracing, and the headline was wrong

Everything above is the HOST's view, and it supported the wrong conclusion. With
tracing added on the ISLAND (a `<Tracing>` block in `kEmbeddedCycloneConfig`,
plus `q_xevent.c`'s two "xmit spdp" `GVTRACE` calls routed to `GVLOGDISC` so the
transmit side lands in the cheap `discovery` category rather than the `trace`
firehose — the firehose over semihosted stdout is slow enough to change the
timing of the thing being measured):

```
1788407870  tev: xmit spdp … (resched 8s)
1788407878  tev: xmit spdp … (resched 8s)
1788407887  tev: xmit spdp … (resched 8s)
1788407895  tev: xmit spdp … (resched 8s)
1788407903  tev: xmit spdp … (resched 8s)
1788407909  tev: xmit spdp … (resched 1s)   <- directed reply to the publisher
1788407911  tev: xmit spdp … (resched 8s)   <- the last one, ever
```

**Seven transmits, on the `tev` thread, at exactly the 8-second cadence the
scheduler computes.** The interval is not absent, the timed-event thread is not
missing, and the config is not at fault — every conclusion in the section above
about a missing announcement is wrong. What happens is that the transmits STOP,
while the island keeps running for another 161 seconds, still logging.

The correlation is tight, and it is not elapsed time:

| time | event |
| --- | --- |
| 1788407909 | publisher reports `matched … =1` |
| 1788407911 | island's LAST spdp transmit |
| 1788407921 | host: `gc: lease expired` → proxy participant deleted |
| 1788407924 | publisher reports `matched … =0` |

Expiry lands 10.3 s after the last transmit — exactly the advertised
`lease_duration` of 10 s. So the "81 seconds rather than 10" puzzle below is
resolved and needs no theory: the lease simply ran from whenever the transmits
stopped. **The data-traffic-renewal speculation in that section is retracted.**

Two findings, and they should not be conflated:

1. **The `tev` thread stops doing its work within ~2 s of the first real traffic
   arriving.** That is the defect. It is a stall, not a missing interval, and its
   trigger looks like load rather than time.
2. **Of seven SPDP packets sent, the host logged one arriving.** Separate, and
   unexplained. Six went missing on a paced local tap, which is its own question
   — and if it were the whole story the lease would still have been renewed by
   the ones that did arrive.

A stalled `tev` also stops PMD heartbeats and lease renewals, which fits the
symptom exactly. Candidates worth separating, in the order they are cheap to
test: `tev` starved or blocked once `recv` / `dq.user` begin real work, or `tev`
overflowing its 16 KiB stack — the same class of bug phase 177.26 fixed for
`recvUC` at 1 KiB, and FreeRTOS's `an536-tasks.py` reports per-task high-water
marks over the QEMU gdb stub.

## What was NOT explained (superseded by the amendment above)

**Why the lease survived 81 seconds rather than 10.** With a 10-second advertised
lease and a single announcement, expiry should land near `+10 s`, not `+81 s`.
The likely reason is that Cyclone renews a proxy participant's lease on receiving
any traffic from it, and the island was publishing control commands for most of
that window — so the lease rode on data, not on SPDP, and expired when the data
stopped. That ordering matters for the fix, because it means SPDP silence may be
the second symptom rather than the first: something may stop the island
transmitting altogether, with the lease expiry following.

Whoever picks this up should establish that ordering before assuming the timed
event is at fault. Two cheap discriminators:

1. Trace with `<Category>discovery,throttle</Category>` on the host and find
   whether island DATA stops at the same instant SPDP would have been due.
2. Trace on the ISLAND side. Everything above is the host's view; nothing here
   proves whether the island sent an announcement that was lost versus never
   sent one. `kEmbeddedCycloneConfig` has no `<Tracing>` block, so that needs
   adding — and on a target with semihosted stdout, sized accordingly.

## Reproduce

`autoware-safety-island` at pin `2b03606ca` or `d2a8955c5` (both reproduce):

```
sudo ip link set tap1 up                     # 192.0.3.1/24, netem 100mbit
ASI_RX_COUNTERS=1 NROS_DOMAIN_ID=2 ./build.sh --platform freertos-an536
# boot the image with -net tap,ifname=tap1, then publish with a
# CYCLONEDDS_URI whose <Tracing><Category>discovery</Category> writes a log
```

The island's own RX counters, read over the QEMU gdb stub
(`scripts/an536-rx-counters.py`), freeze at the deletion and never advance:

```
                t0        t+20s
asi_rx_traj     154        154
asi_rx_odom     635        635
asi_rx_accel    613        613
asi_rx_steer    612        612
```

Confirm the guest is still running before believing that — gdb halts the target
on attach, and a failed resume produces the same frozen numbers for an entirely
different reason. Here the island's log kept advancing across the reads.
