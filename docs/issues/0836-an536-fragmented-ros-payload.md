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

## Still unknown

Where the sample dies between the NIC and the reader: lwIP pbuf exhaustion
mid-burst, Cyclone's defragmentation buffers
(`Internal/DefragReliableMaxSamples`, `Sizing/ReceiveBufferSize`), or the
reliable repair path failing to make progress over this link. The next
measurement is a fragment-level count at `lan9118_lwip_poll()` compared
against Cyclone's `dds_get_status` / rejected-sample counters — establishing
whether the fragments reach lwIP at all decides which half to look in.

## Why it matters

This is the last thing between the emulated Cortex-R52 lane and a full
closed-loop Autoware demo. Everything else in that loop now works: the image
boots, discovers a real ROS 2 graph, publishes control commands Autoware
accepts, and autonomous mode engages. The vehicle does not move only because
the controller has no trajectory to follow.

It also generalises past this board. Any FreeRTOS/lwIP nano-ros image talking
to real ROS 2 will meet a topic bigger than one datagram; the small-message
examples in this repo never do, which is why the whole class stayed invisible.
