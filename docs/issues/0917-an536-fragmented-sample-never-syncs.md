---
id: 917
title: "A fragmented sample either syncs at once or never arrives — 8 RTPS
  fragments reach the reader in 2 runs of 6, and a failed run never recovers"
status: open
type: bug
area: rmw, platform
related: [issue-0836, issue-0889, issue-0749]
---

## Symptom

On `mps3-an536-freertos` with a real ROS 2 peer, a subscription whose samples
span several RTPS fragments delivers **nothing at all** in most runs of the
same binary, while a single-datagram sample on the same participant, same
link, same run delivers every time.

Six trials per size, identical conditions — one host participant publishing
three small topics at 40 Hz plus one `autoware_planning_msgs/Trajectory` at
10 Hz, no Autoware, no bridge:

| payload | serialized | fragments | reader got it |
| --- | --- | --- | --- |
| 10-point trajectory | 908 B | 1 | **6 of 6** |
| 120-point trajectory | 10 588 B | 8 | **2 of 6** |

The failure is **binary, not degraded**. In the two runs where the
8-fragment stream came up, it stayed fresh for the rest of the run (zero
staleness against a 0.5 s freshness check). In the other four the sample never
arrived once in 25 s of a 10 Hz publisher — roughly 250 samples, ~2000
fragments, all sent. There is no middle state where some samples land.

So this is not a throughput ceiling. Something has to succeed once, early, and
when it does not the reader stays stuck for the life of the image.

## Where it is NOT

Measured, not argued:

* **Not the link, and not below Cyclone.** With `LWIP_STATS` on, under this
  exact load: every pool `err=0`, `TCPIP_MSG_INPKT` and `PBUF_POOL` peak at 16
  of 64 and 16 of 128, `udp.recv` 3991 and climbing. ICMP answers throughout.
  (Contrast issue 0836, which WAS an lwIP pool starving — that one is fixed and
  this is what is left.)
* **Not the socket handoff.** netconn recvmboxes read empty with `rcvevent=0`
  while the failure is in progress: Cyclone's `recv` thread is draining the
  socket as fast as lwIP fills it.
* **Not the fragments failing to be sent.** The wire carries 1416 `DATA_FRAG`
  and 1239 `HEARTBEAT_FRAG` host→island in a failing run.
* **Not Cyclone's receive-buffer sizing.** `Sizing/ReceiveBufferSize` 64 KiB /
  chunk 16 KiB → 1 MiB / 128 KiB changes nothing (tested against this failure
  specifically; the earlier test of that knob in 0836 was against the discovery
  failure, not this one).
* **Not a broken NACKFRAG path.** At a low publish rate the reader does send
  them — 17 `NACKFRAG` in a traced 120-point run. The machinery works.

## What the island's own trace shows

`<Tracing><Category>radmin</Category>` on a failing run:

```
recv:   defrag_rsample_drop (0x217135d0, 0x214b3a08)
recv:   defrag_rsample_drop (0x217135d0, 0x214b5e68)
recv:   defrag_rsample_drop (0x217135d0, 0x214b82c8)
    ...  33 drops, all on the same defrag admin
```

and on the wire in that same state the island sends **zero** `NACKFRAG` — so
partial samples accumulate in the defrag admin, hit `DefragReliableMaxSamples`
(default 16), and are evicted before anything repairs them. Each new sample
evicts an older partial one and nothing ever completes: a self-sustaining
state, which is why a failed run never recovers.

The open question is what stops the reader from asking for the fragments it is
missing in this state, given that the same reader does ask at a low rate.

## Reproduction

In the ASI consumer (`autoware-safety-island`), which is where the rig lives:

```
scripts/an536-sweep-pub.py --small-only --with-trajectory 120   # 8 fragments
scripts/an536-sweep-pub.py --small-only --with-trajectory 10    #  1 fragment
```

against a booted `mps3-an536` island on a tap, and read the controller's
`Waiting for trajectory data` line. Publish the trajectory from the SAME node
as the small topics: a second process is a second participant, and a
late-joining participant has its own intermittent discovery failure that
produces an identical-looking log.

Expect to repeat: single runs are not a measurement on this lane.

## Why it matters

Every ROS 2 graph has a topic bigger than a datagram — this one is an Autoware
trajectory, and the controller holds a safe stop without it, so the vehicle
never moves. The repo's own examples publish small fixed-size messages and
never reach a second fragment, which is why the whole class stays invisible in
CI.
