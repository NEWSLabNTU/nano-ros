---
id: 2970
title: "An XRCE session never noticed a DDS participant leave when the participant's last ros_discovery_info sample did not reach it"
status: open
type: bug
severity: high
area: [rmw-xrce, graph, interop]
related: [issue-2106]
found: 2026-11-25
---

## Symptom

On live-peer run 45596621496 (main at cffdca5c9b),
`xrce_ros2_interop::xrce_a_peer_leaving_fires_the_graph_change_guard` failed.
The stock talker exited cleanly on SIGTERM, but the XRCE graph probe's
graph-change guard did not fire within 20 s:

```
GRAPH_PROBE_CHANGE_ARMED 2
GRAPH_PROBE_CHANGE_NONE
GRAPH_PROBE_FAIL: no graph change within 20000 ms of arming at 2
```

## Not a regression

- **Pass history.** The cell passed in all 15 live-peer runs that reached it,
  from 2026-11-18 to 939ac840b2, each in 1.2 to 2.7 s. The three reds on
  2026-11-24 (runs 45358237792, 45370439146 and 45371554201) were
  `fixture not built`, with all eight XRCE tests red.
- **What changed in between.** The commits between 939ac840b2 and cffdca5c9b
  are the issue 2965 pair, a download retry, a max-frame check and a census
  CLI fix. None is on the probe's path. The 2965 band waiter is reached only
  by a spawned level, and the probe is one Rust executor that spawns none.
- **Reproduced at both ends.** Pinning the whole test to one CPU core with
  three busy loops on it reproduced the exact CI signature. It failed 4 of 10
  runs at cffdca5c9b and 3 of 10 at 939ac840b2.
- **Not slow, lost.** Unloaded, 5 of 5 runs passed. The talker's process
  group exits about 50 ms after SIGTERM, and the edge reaches the probe at 102
  to 119 ms. Both numbers are quantized by the harness's 50 ms poll, against a
  20 s window. The sample never arrives at all.

## Where the sample is lost

The repro was run by hand on host 139 with four processes: the stock talker
(`rmw_fastrtps_cpp`), the XRCE Agent, the XRCE graph probe, and our Cyclone
graph probe. The Cyclone probe runs on free cores and reads the same
`ros_discovery_info` samples straight off DDS, with no Agent in its path, so
it is an independent observer. Ten runs per placement:

| On the contended core | XRCE probe fired | Cyclone probe fired |
|---|---|---|
| talker, Agent and XRCE probe | 7/10 | 10/10 |
| the Agent alone | 4/10 | 10/10 |
| the XRCE probe alone | 10/10 | 10/10 |

- The talker does publish its departure.
- Our client keeps up even when it is the starved process.
- When the **Agent** is CPU-starved, the departing writer's last sample is
  lost inside it. It never comes out of the Agent's DDS reader onto the XRCE
  stream.

It happened with the pinned SDK Agent (Fast-DDS 2.14.6) and with CI's
ROS-paired Agent (Fast-DDS 2.6).

Two hypotheses were ruled out:
- **Asynchronous publishing.** Humble's rmw_fastrtps publishes asynchronously
  by default. Setting `RMW_FASTRTPS_PUBLICATION_MODE=SYNCHRONOUS` on the
  talker failed 7 of 10, so it is not the cause.
- **Our client.** Starving it alone failed 0 of 10.

The likely mechanism, not proven, is the Agent's Fast-DDS reader handling the
writer's removal before the sample was taken, which discards it.

## The product gap underneath

The XRCE backend learns the ROS 2 graph only from `ros_discovery_info` data
samples (issue 2106). XRCE carries data samples and never DDS participant or
endpoint removal. So a departure was seen only through the participant's last
sample, the empty node list a graceful exit publishes.

Nothing guarantees that sample arrives. ROS 2 itself does not:
- a SIGKILL or a crash never publishes it;
- a dropped sample is never repaired once its writer is gone.

In every such case the participant stayed listed, and the graph-change event
never fired, until the session ended. DDS RMWs have participant removal as the
backstop (a lease expiry or a dispose). XRCE had none.

## Fix: the graph re-sync

The fix is in `packages/rmw/xrce/nros-rmw-xrce/src/graph.c`, section "the
re-sync".

**How it works:**
- The `ros_discovery_info` reader is TRANSIENT_LOCAL, and every live
  participant's writer keeps its current sample.
- So a freshly created reader is handed one sample per participant that is
  still there, and none for a participant whose writer the Agent has removed.
- Every `XRCE_GRAPH_RESYNC_MS` the session re-creates the reader in place
  (`UXR_REPLACE` on its own object id) and starts a new generation.
- Each sample stamps its participant with the generation that delivered it.
- At the next re-sync, a participant the confirmed generation did not
  re-deliver is dropped, and the graph-change event fires.
- The re-create is confirmed through the session's status callback, so the
  spin never blocks on the Agent.
- The re-created reader is one more DDS endpoint on the participant, so it
  claims one endpoint key, which keeps issue 1292's GID count exact. A refused
  re-create leaves the count unknown until the next probe.

**The knob.** `xrce.graph_resync_ms` (`NROS_XRCE_GRAPH_RESYNC_MS`,
`CONFIG_NROS_XRCE_GRAPH_RESYNC_MS`) sets the period. The default is 5000, and
0 turns the re-sync off. It is carried on the cargo, CMake and west roads
(`knob_road_coverage.tsv`).

**Bound.** A departure is seen within 2 × the period: the first re-sync after
it starts a generation that cannot re-deliver the participant, and the next
one sweeps it. A crash adds the time the Agent's DDS participant takes to
notice, its lease (20 s by Fast-DDS default). A graceful exit whose last
sample arrives is still seen at once.

**Cost.** The re-sync runs only once the reader exists, which happens on the
first graph query or graph-change registration. An image that never reads the
graph pays one flag test per `drive_io` and sends nothing extra. Once the
reader exists, each period costs:
- from the client: one CREATE (datareader, REPLACE) and one READ_DATA;
- from the Agent: one STATUS, and one re-delivered `ros_discovery_info` sample
  per live DDS participant;
- on the Agent: one DataReader deleted and created.

The reader's depth was raised to the peer cap (`XRCE_GRAPH_MAX_PEERS`, 32).
The re-delivered samples share one keyless history, and a burst must not push
a live participant out.

**What it cannot do.** A participant whose re-delivery takes longer than a
whole period to reach the client is dropped, then re-added when its sample
arrives, firing the event twice.

## Measured after the fix (host 139)

- **Agent alone contended** (placement 2 above, 4/10 before): 10 of 10 fired.
  - Five runs had the last sample arrive and fired at 104 to 107 ms.
  - Five lost it and were caught by the re-sync at 9.0 to 10.1 s, inside
    2 × 5 s plus one spin tick.
- **Whole interop test pinned to one contended core** (4/10 before): 10 of 10
  passed. Three runs used the re-sync, at 9.2 to 10.0 s.
- **Solo**: 5 of 5 passed, with the edge at about 102 ms.
- **Unit tests** (`tests/graph_resync.c`):
  - a participant the new generation did not re-deliver is dropped, and one it
    did is kept;
  - the sweep fires the event once;
  - the re-create's status claims one key;
  - a refused re-create confirms nothing;
  - nothing happens before the period is due, or without a reader.

The interop test's window is now derived from the bound:
`2 × XRCE_GRAPH_RESYNC_MS + 2 s` = 12 s (`XRCE_LEAVE_WINDOW_MS`, which writes
down what the margin covers).

## Upstream

Not worth reporting yet. The loss is inside the Agent, but these runs do not
separate the Agent's own read loop from Fast-DDS discarding a removed writer's
untaken sample. If it is the latter, a plain Fast-DDS reader would show the
same loss, and it is arguably intended DDS behaviour. ROS 2 makes no delivery
promise for the last sample either. A report would first need the same
starvation repro against a bare Fast-DDS reader. Either way, the fix here
holds, because the re-sync does not depend on the last sample.
