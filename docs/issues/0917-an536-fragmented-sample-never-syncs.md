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

## Confirmed by intervention: widen the RX FIFO and it goes away

The FIFO was rebuilt from the pinned fork with the data FIFO at 16384 words
(64 KiB, up from 2640 / 10 560 B) and the status FIFO at 1024 entries (up from
176), nothing else changed — same image, same rig, same host publisher:

| RX FIFO | runs | trajectory arrived | fresh | faults |
| --- | --- | --- | --- | --- |
| 2640 words (10 560 B, stock) | 6 | 0-1 | never | 1 |
| 16384 words (64 KiB) | 6 | **6** | **6** | **0** |

Every run: `never=0 stale=0 faults=0`. The first time this lane has held a
fragmented sample AND kept it fresh. A 200-point sample (17 628 B, 13
fragments) also arrives, 2 of 3 — the third run lost discovery, which is the
separate intermittent failure noted above and not a size effect.

**The full Autoware demo agrees.** With the widened FIFO, every input's
"waiting" count freezes at boot (7/7/7/8) and `Waiting for fresh trajectory
data` **stops growing at 2** instead of climbing for the whole run, with zero
faults. That is the state the controller needs to stop withholding commands.

Note this also takes the intermittent stack overflow with it across these runs
(0 faults in 6, where the stock FIFO produced them regularly). Consistent with
that fault being a consequence of the perpetually-backed-up receive path rather
than an independent bug — worth re-checking rather than assuming.

The patch is three constants that must move together (the data FIFO is a fixed
array, so the array, its vmstate descriptor and the runtime size are one
change):

```
-    uint32_t rx_fifo[3360];
+    uint32_t rx_fifo[16384];
-        VMSTATE_UINT32_ARRAY(rx_fifo, lan9118_state, 3360),
+        VMSTATE_UINT32_ARRAY(rx_fifo, lan9118_state, 16384),
-    s->rx_fifo_size = 2640;
+    s->rx_fifo_size = 16384;
-    s->rx_status_fifo_size = 176;
+    s->rx_status_fifo_size = 1024;
```

**Landing it is a maintainer action, not an agent one.** It belongs on the
qemu fork's patch branch, and the SDK ships qemu as a prebuilt dist
(`nros-sdk-index.toml`), so it needs a fork push plus a new dist build before
any consumer sees it. Until then the an536 lane cannot carry an Autoware-sized
fragmented topic.

One caveat to state plainly: this widens an emulated part beyond what the real
LAN9118 has (the hardware FIFO is ~10 KiB shared, which is where 2640 words
comes from). That is defensible for a lane whose purpose is to run real ROS 2
traffic against a Cortex-R52 image whose actual silicon does DMA rather than a
FIFO — but it is a deliberate divergence from the modelled part and should be
recorded as one, not slipped in as a bug fix.

## Interrupt-driven RX was built and measured, and it is NOT the fix

Worth recording in full, because it is the obvious next move and it does not
work.

**Built end to end** (patch kept out of tree; ~156 lines):

* driver — `lan9118_lwip_rx_irq_enable` / `_mask` / `_rx_pending`. RSFL is a
  LEVEL condition (it asserts while the RX status FIFO is non-empty), so the
  ISR masks rather than clears and the drain task re-enables once the FIFO
  reads empty. `IRQ_CFG` was already programmed `IRQ_EN|IRQ_POL|IRQ_TYPE` by
  `lan9118_lwip_init`; only `INT_EN` was left at 0.
* board — GICv3 SPI wiring for the ethernet line. `lan9118_init(0xe0300000,
  qdev_get_gpio_in(gicdev, 18))` in `hw/arm/mps3r.c` means SPI 18, INTID **50**.
  SPIs live in the DISTRIBUTOR, not the redistributor frame the timer PPI uses,
  and with `GICD_CTLR.ARE_NS` set an SPI also needs an `IROUTER` entry or it is
  enabled and targets nobody.
* glue — ISR masks, `vTaskNotifyGiveFromISR`, drain task waits with
  `ulTaskNotifyTake`. The poll interval becomes a CEILING rather than the
  cadence, and the task drains FIRST and waits second.

**One ordering trap worth writing down.** Enabling the SPI in `gicv3_init()`
hangs the image before its first print. The model treats the interrupt output
as active-LOW until the driver sets `IRQ_POL|IRQ_TYPE`, so between reset and
`lan9118_lwip_init()` the line is held ASSERTED — the interrupt is taken
immediately, against a netif that does not exist yet. Enable the GIC side only
after the driver has configured the source.

**It works, and it does not help.** The ISR fires (464 times in a 35 s run,
zero spurious IDs), and delivery is **2 of 6** — the same as the unmodified
build, while simply polling every 1 ms instead of every 5 ms measured 9 of 11.

The counter says why: 464 ISRs over 35 s is ~13/s against ~130 packets/s
arriving. The FIFO rarely empties, so the mask stays on and the 5 ms timeout
does the work anyway. Wake latency was never the binding constraint — **the
FIFO is**. The guest cannot be scheduled between the frames of one burst at
all, so nothing on the guest side changes how much of a burst fits.

That also explains why 1 ms polling beat it: it does not make the guest react
faster to a burst, it keeps the FIFO emptier on average so more of the next
burst fits.

## Directions that remain

* **Keep the cheap half regardless.** Drain-first/wait-second with the poll
  interval as a ceiling is strictly better than sleep-first, and costs nothing.
* **Widen the FIFO.** nano-ros pins its own QEMU fork, so `rx_fifo_size` (2640
  words) is ours to change for this lane. Note the lane currently runs the
  SYSTEM qemu (9.0.2) — the fork is not built — so this needs the rig pointed
  at the fork first.
* **Accept the limit.** On real hardware the RX path is DMA to memory, not a
  10.5 KB on-chip FIFO, so this specific ceiling is an artifact of the emulated
  board rather than a product defect. Interrupt-driven RX is still the right
  shape for real silicon and removes a fixed 5 ms latency floor — it just does
  not close THIS gap.

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
