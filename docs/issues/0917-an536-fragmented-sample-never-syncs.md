---
id: 917
title: "The emulated LAN9118 RX FIFO cannot hold an 8-fragment RTPS burst,
  and a 5 ms RX poll drains it far too late"
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
* **Not the writer's history depth.** `KEEP_LAST(1)` lets the writer drop a
  sample the reader is still reassembling, which would explain a stall that
  never repairs; at depth 20 it is still 2 of 6.
* **Not the subscription buffer above Cyclone.** `NROS_SUBSCRIPTION_BUFFER_SIZE`
  64 KiB → 256 KiB, no change.
* **Not the take/deserialize path above Cyclone at all.** With `rhc` tracing and
  the small topics slowed to 0.5 Hz so the streams are separable, a failing run
  logs 16-20 `rhc_store` — about what the small topics alone account for, where
  a delivered 10 Hz trajectory would add ~250. The sample never reaches the
  reader history cache, so it is lost in reassembly and not after it.

## Root cause

**The burst does not fit in the NIC, and the guest drains it too late.**

QEMU's LAN9118 model sizes its receive FIFO in WORDS:

```c
s->rx_fifo_size = 2640;              /* uint32_t rx_fifo[3360] */
...
static bool lan9118_can_receive(NetClientState *nc) {
    ...
    /* Leave a frame's worth of headroom in the data FIFO. */
    return s->rx_fifo_size - s->rx_fifo_used >= 384;
}
```

2640 words is **10 560 bytes**, and a frame is refused unless 384 words
(1536 B) are free. A 10 588-byte sample goes out as 8 RTPS fragments of
1344 B payload, ~1420 B on the wire — **~11 360 bytes of back-to-back frames,
which cannot fit**. Six fit comfortably; the seventh is marginal against the
headroom rule; the eighth never has room.

The guest's side of it is `poll_task_entry`: `vTaskDelay(poll_interval_ms)`
then drain at most 16 packets. `poll_interval_ms` is **5** and the tick is
1000 Hz, so the RX FIFO is emptied every 5 ms while the burst lands in a small
fraction of that. Nothing drains between the frames of one sample.

The island's own defrag trace says exactly this. Contiguous reassembly of the
10 588-byte sample reaches:

```
    159 [0..2688)      2 fragments
    159 [0..4032)      3
    159 [0..5376)      4
    159 [0..6720)      5
    159 [0..8064)      6   <- 159 samples get this far
      3 [0..9408)      7   <- three ever get this far
      0 [0..10588)     8   <- none, ever
```

A hard cutoff at **6 fragments / 8064 bytes**, 159 times out of 159. That is
not random loss, it is a capacity limit: 6 frames is what the FIFO holds once
the 1536-byte headroom rule is applied.

It also explains the whole earlier shape. One-datagram samples never touch the
limit (6 of 6). Two-to-four fragment samples fit, and arrive. Eight-fragment
samples lose their tail every time, so no sample ever completes, the defrag
admin fills with partials and evicts them, and the reader never recovers.

## Mitigation, measured

Polling every tick (1 ms) instead of every 5 ms, same binary otherwise:

| RX poll interval | valid runs | delivered |
| --- | --- | --- |
| 5 ms (`poll_interval_ms` default) | 6 | 2 |
| 1 ms | 11 | **9** |

Better, and not a fix: a burst can still outrun any fixed cadence, because the
guest is not scheduled between the frames of one sample at all. The directions
that actually close it are interrupt-driven RX (the model does raise RX
interrupts) or a drain that keeps going while packets are pending instead of
sleeping a fixed interval — the poll task currently sleeps FIRST and drains
second, so the interval is a floor on how long a full FIFO waits.

Worth noting for whoever picks this up: `LINK_STATS` reads all zero because the
lan9118 driver never calls the lwIP link-stat macros, so loss at exactly this
layer is invisible to `lwip_stats`. Every pool below it reads clean while the
NIC is dropping the tail of every burst.

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
