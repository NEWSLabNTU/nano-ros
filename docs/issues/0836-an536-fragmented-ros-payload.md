---
id: 836
title: "A FreeRTOS/lwIP image receives every small ROS topic and never a
  fragmented one — a 13 KiB Autoware trajectory never arrives"
status: open
type: bug
area: rmw
related: [phase-385, issue-0749, issue-0830]
---

## Symptom

An `mps3-an536-freertos` image running the Autoware Safety Island controller,
on a `tap` with a real ROS 2 / Autoware stack (DDS domain 2, `domain_bridge`
relaying six topics from Autoware's domain 1), receives **every small topic and
never the large one**:

| topic | serialized size | reaches the image |
| --- | --- | --- |
| `/localization/kinematic_state` | ~700 B | yes, 40 Hz |
| `/vehicle/status/steering_status` | ~50 B | yes |
| `/localization/acceleration` | ~200 B | yes |
| `/system/operation_mode/state` | ~50 B | yes |
| `/planning/scenario_planning/trajectory` | **~13 KiB** | **no** |

The controller logs `Waiting for trajectory data` indefinitely while it has
everything else, so it holds a safe stop and the vehicle never moves.

This is not the bridge and not the wire. A host CycloneDDS subscriber on the
SAME domain, SAME interface and SAME topic reads a clean 10 Hz throughout:

```
$ ROS_DOMAIN_ID=2 ros2 topic hz /planning/scenario_planning/trajectory
average rate: 10.000  min: 0.097s max: 0.102s std dev: 0.00056s window: 232
```

The distinguishing property is SIZE: 13 KiB against the peer's
`MaxMessageSize` of 1400 B is ~10 RTPS fragments per sample, where every topic
that works fits in one datagram.

## Ruled out

* **Reader QoS.** The reader is RELIABLE (nros `QoS::default_profile()`), so a
  dropped fragment should be NACKed and repaired rather than losing the sample.
* **Subscription buffer.** Raised 16 KiB → 64 KiB
  (`NROS_SUBSCRIPTION_BUFFER_SIZE`, the knob issue 0749 is about). This moved
  the count off zero — the controller went from "no trajectory" to "trajectory,
  but not fresh" — so the buffer WAS one constraint, and is not the last one.
* **lwIP receive sizing.** The family defaults are sized for an MPS2-AN385
  (UDP receive mbox 8, pbuf pool 24, 32 KiB heap) — smaller than one burst.
  Raised on this board (this commit): mbox 64, pool 128, 256 KiB heap. With
  it, Autoware's operation-mode manager accepts the island's commands and
  autonomous mode ENGAGES, which it never did before. The trajectory still
  does not arrive.
* **The clock.** Fixed separately and independently (the image had no wall
  clock at all); the trajectory behaviour is identical before and after.

## Measured 2026-08-28 — the fragments DO reach lwIP

Re-checked at main `60b4e0c1e` (the 100+ commits since the report changed
nothing here): still open, still reproduces.

Instrumenting BOTH ends of the suspect path in one run — a frame/byte counter
inside `lan9118_lwip_poll()`, and an arrival counter in the consumer's
trajectory callback — splits the two halves the issue could not choose between:

```
RXSTAT frames=3200 bytes=1762696 big=1148     <- NIC, frames >800 B counted as "big"
TRAJ arrivals: 5                              <- samples the subscriber actually got
```

So the wire and the NIC are NOT innocent bystanders and NOT the whole story:

* **Fragments do arrive.** 1148 large frames reached lwIP. The earlier
  hypothesis that the burst never got past the NIC is wrong.
* **But most are missing.** A 13 KiB sample at 10 Hz is ~100 large frames per
  second; over a ~100 s run that is ~10,000. We saw 1148 — roughly **11%**.
* **Which is why samples almost never complete.** With ~89% of a burst missing,
  a reliable reader spends its time NACKing rather than delivering: 5 samples
  in a run, against ~1000 published.

The loss is therefore between the wire and the driver's drain — the NIC's RX
FIFO, or the cadence that empties it — not in Cyclone's reassembly of frames it
already holds.

### The drain path, and what did not fix it

`lan9118_lwip_poll()` drains a bounded 16 frames per call; the FreeRTOS poll
task calls it every `poll_interval_ms` (5 ms on this board). One 10-fragment
burst therefore has to survive in the LAN9118's RX FIFO (~10 KB after the 5 KB
TX allocation) until the next poll.

Raising the budget 16 -> 128 did NOT help: that run recorded FEWER frames (600)
and zero arrivals. Do not read that as "128 is worse" — run-to-run variance on
this lane is large enough (3200 vs 600 frames across two runs of the same
length) that single runs cannot separate a real effect from noise. **Any future
attempt here needs repeated runs, not one.**

### Next

Read the LAN9118's own dropped-frame counters (`RX_DROP`) to confirm FIFO
overflow directly rather than inferring it from the deficit, and if confirmed,
compare interrupt-driven RX against faster polling. The board registers its
netif in POLL mode (`nros_board_poll_netif`), which was the cheapest shape for
bring-up and may simply not survive a real ROS burst.
## Correction: this image does NOT run a flow-control-less LAN9118

Reasoning here kept starting from `docs/research/qemu-lan9118-slirp-rx-stall.md`
and its finding that "LAN9118 registers no `.can_receive` and never calls
`qemu_flush_queued_packets`, so a full RX FIFO drops frames outright". That is
true of **stock** QEMU and false of the binary this board runs: the SDK ships
`11.0.0-nros2`, built from `NEWSLabNTU/qemu` branch `nano-ros-v11.0.0-patches`,
which carries our own `can_receive` patch (`nm` finds the symbol; the
disassembly is quoted in issue 0830). The throttle EXISTS. So the question is
whether its re-arm cycle keeps up across a burst, not whether back-pressure is
present — and given the measurement above puts the loss at the drain, that
cycle is now the prime suspect rather than a detail.

## The RX FIFO number, and one correction to it

`lan9118_reset` fixes the RX FIFO at `s->rx_fifo_size = 2640` **words**
(10,560 bytes), and each frame costs `(size + n + 3) >> 2` words plus one for
the CRC — about **352 words for a 1400 B fragment**, so ~7.5 fragments fit. A
10-fragment burst needs ~3520 words and cannot be resident at once.

The ~10 KB figure above is right, but **not because of the 5 KB TX
allocation**. On silicon the 16 KB FIFO is split between TX and RX, so
`TX_FIF_SZ = 5` in `hw_init` would shrink RX — but QEMU does not model the
split: `case CSR_HW_CFG` stores the bits and never recomputes `rx_fifo_size`.
The size is hardcoded at reset and identical whatever `hw_init` writes. Worth
knowing before anyone "fixes" this by lowering the TX allocation: on QEMU that
changes nothing, and on real hardware it would.

## Also ruled out

* **QEMU net-queue depth.** `nq_maxlen` is 10000 and the drop-on-full path
  (`net/queue.c:102`) requires `!sent_cb`; tap always passes
  `tap_send_completed`. Frames are not dying in the queue.

## A drain-path suspect in our own driver

Given the loss is between the wire and the drain, this is worth checking before
anything in QEMU. `lan9118_lwip_poll` stops on any `NULL`, but `rx_receive`
returns `NULL` for four different reasons — no packet, error status, bad
length, and **`pbuf_alloc` failure** — and the last three have already consumed
or discarded a frame with more still pending:

```c
struct pbuf *p = rx_receive(base);
if (p == NULL)
    break;          /* <- also taken when a frame was merely discarded */
```

Under exactly the burst this issue is about, pbuf pressure is highest, so an
alloc failure mid-burst ends the drain early — leaving the FIFO fuller for
longer, which is the condition that makes `can_receive` stay false and the
backend stop reading. That is a plausible mechanism for losing most of a burst
while the first frames of it arrive fine. Distinguishing "FIFO empty" (stop)
from "frame discarded" (continue within budget) is a two-line change, and the
`pbuf_alloc` failure count is the counter to add alongside the `RXSTAT` ones.

## Still unknown

WHY ~89% of each burst is lost below the driver. The fragment count above
settled the half-question this section used to pose — they do reach lwIP — so
what remains is the RX FIFO / drain cadence, not Cyclone's defragmentation.

## Why it matters

This is the last thing between the emulated Cortex-R52 lane and a full
closed-loop Autoware demo. Everything else in that loop now works: the image
boots, discovers a real ROS 2 graph, publishes control commands Autoware
accepts, and autonomous mode engages. The vehicle does not move only because
the controller has no trajectory to follow.

It also generalises past this board. Any FreeRTOS/lwIP nano-ros image talking
to real ROS 2 will meet a topic bigger than one datagram; the small-message
examples in this repo never do, which is why the whole class stayed invisible.
