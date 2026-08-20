---
id: 705
title: "`case_08_c_qos` in-sweep: `ros2 topic info` sees ANOTHER test's `talker` on /chatter and not this cell's own `qos_talker`, despite a per-cell router"
status: resolved
type: bug
area: testing
related: [issue-0690, issue-0309, issue-0312, phase-263, rfc-0051]
---

## Symptom

In a tier-1 sweep (in the ROS distrobox, the only place this assertion runs):

```
[c qos] ros2 discovered no PUBLISHER named `qos_talker` on /chatter
Type: std_msgs/msg/Int32

Publisher count: 1

Node name: talker
Node namespace: /
Endpoint type: PUBLISHER
QoS profile:
  Reliability: RELIABLE
  History (Depth): KEEP_LAST (10)
  Durability: VOLATILE
```

Two facts, and the second is the one that matters:

1. The single publisher visible on `/chatter` is a node called **`talker`**. No
   cell in `workspace_features_e2e` names a node `talker` — the qos cells all
   name theirs `qos_talker` (`demo_bringup/launch/*qos*.xml`). It belongs to a
   DIFFERENT test.
2. **`Publisher count: 1`.** This cell's own `qos_talker` is not merely
   outranked in the report, it is ABSENT. The `ros2` CLI was looking at a graph
   that does not contain the process this test started.

## This is what issue 0690 could not see

0690 was "the profile assertion reads the FIRST endpoint block, which need not
be the one under test". Its fix (`topic_endpoints_for_node`) is working exactly
as intended and is what produced the message above: instead of silently
asserting against a foreign endpoint's VOLATILE profile and reporting "the
per-entity profile was dropped between the node and the wire", the cell now
says which node it could not find. The hypothesis 0690 recorded — a foreign
publisher on the topic — is CONFIRMED.

What 0690 assumed, and what is now falsified, is that the foreign endpoint was
an EXTRA one sharing the view. It is the only one. So this is not a reporting
defect at all; the cell is talking to the wrong graph.

## Why that is surprising

Every cell starts its own router:

```rust
let router = ZenohRouter::start_unique()   // workspace_features_e2e.rs
```

and the `ros2` CLI is pointed at it through a per-invocation session config
(`ros2_env_setup_with_locator` writes `session_config.json5` into a TempDir and
exports `ZENOH_SESSION_CONFIG_URI`). Both halves are per-cell, so a foreign
node should not be reachable and the local one should not be missing.

## Not yet established

The mechanism. Two candidates, neither confirmed:

* **Port lease TOCTOU.** `start_unique()` calls `lease_ephemeral_port()` and
  then starts zenohd on that port. If the lease is released before zenohd
  binds, a concurrent test can take it, and this cell's CLI then dials a router
  belonging to someone else — which would produce exactly "their node, not
  mine, count 1".
* **Scouting.** If the generated session config leaves multicast scouting on,
  sessions can find peers beyond the configured endpoint. This explains a
  foreign node appearing but NOT the local one being absent, so on its own it
  does not fit.

The first fits every observed fact and should be checked first. Note
`ros2_env_setup_with_locator` sets `RMW_IMPLEMENTATION` and
`ZENOH_SESSION_CONFIG_URI` but no `ROS_DOMAIN_ID`, so domain separation is not
doing any work here either.

## Reproduction

In-sweep only, and intermittent — the history across identical trees is
sweep FAIL / solo PASS / sweep PASS / sweep FAIL. Solo it passes because no
other test is holding a router.

```
just ci          # in the ROS distrobox; ~1 in 2 sweeps
```

## Why it only shows up here

The profile assertion is gated on `require_ros2()`, so it runs only where ROS 2
exists — in this repo, only inside the distrobox. Any host without ROS skips it
and cannot see this at all.

## Fix direction

Establish which mechanism it is before changing anything; the two want
different fixes (hold the port until zenohd owns it, versus pinning scouting
off in the session config). A cheap discriminator: have the cell log the
router port it leased and the port its `ros2` invocation dialled, and compare
them on failure — if they differ, it is the lease.

Do NOT respond by loosening the assertion. The cell is correctly reporting that
it cannot see its own node; that is a real property of the run.

## Fixed 2026-08-20 — the query sampled discovery once; it now polls

The mechanism was neither candidate. Both were about the cell reaching the
WRONG graph; the truth is simpler and the evidence pointed at it all along.

`lease_ephemeral_port()` already holds an `O_EXCL` lock for the router's
lifetime (issue 0470, and `ZenohRouter._lease` keeps it), so the port-lease
TOCTOU cannot happen between our own fixtures — that candidate is dead. And
scouting never fitted: it explains a foreign node appearing, not the local one
being absent.

What is left is timing. The cell ran `ros2 topic info` ONCE, immediately after
the third sample arrived. ROS 2 discovery is eventually consistent, so a
liveliness token reaching that particular `ros2` invocation is a separate event
from delivery working — and under sweep load it lags. The foreign `talker` was
already in the graph because its test had been running for a while; ours had
not propagated yet. `Publisher count: 1` was not "the wrong graph", it was "the
graph so far".

So the query polls until this cell's own endpoints appear, up to 20 s, and only
then asserts. This does NOT weaken anything: it still requires OUR node by name
(issue 0690's selection), still asserts the full declared profile, and on
timeout fails exactly as before with the last report. More than one match still
fails immediately rather than being waited out — that is the sibling-cell case,
and waiting would only grow the report.

The failure message now says it polled and that delivery already passed, so the
next reader is not sent looking at wiring.

### Verified

A full tier-1 sweep in the ROS distrobox: **1480 cases, 0 real failures**,
`case_08_c_qos` among them. Against a prior in-sweep rate of roughly 1 in 2
that is meaningful but not conclusive — one green sweep cannot prove a flake
gone, and the honest claim is that the race it removes is the one the evidence
describes.

Solo runs prove nothing here and were not used as evidence: 3/3 rounds of the
three qos cells passed before the fix as well.

