---
id: 1333
title: "The ros2cli daemon is keyed on ROS_DOMAIN_ID alone, so it serves a graph
  computed under ITS discovery config, not the caller's"
status: resolved
type: bug
area: testing
severity: medium
related: [issue-1009, issue-1137, issue-0763, issue-0707, issue-0741]
---

## Summary

`ros2 node list` / `ros2 param list` / `ros2 topic list` can report an empty
graph, or `Node not found`, for a node that is live and reachable on that
domain — and the identical command with `--no-daemon` finds it. The daemon is
not stale in TIME; it is answering from a different BUS.

This cost one investigation of a parameters example roughly forty minutes and
**four consecutive false negatives**, each read as "the example is broken" when
the example was fine. That is the issue-1009 class: an environment fact that
makes a correct system report as broken, where the wrong diagnosis is more
expensive than the bug would have been.

## What was originally reported, and what is actually true

The finding was reported as "the daemon caches the graph **across ROS
domains**". Measured, that is not what happens, and the difference matters
because it changes the remedy.

ros2cli derives the daemon's port from the domain and nothing else
(`/opt/ros/humble/lib/python3.10/site-packages/ros2cli/daemon/__init__.py`):

```python
def get_port():
    base_port = 11511
    base_port += int(os.environ.get('ROS_DOMAIN_ID') or 0)
    return base_port
```

So the domain IS in the key, by port arithmetic, and it is injective. Measured:
a daemon primed on domain 71 does not serve a query on domain 72 — domain 72
spawns its own daemon on 11583 and answers correctly.

What is **not** in the key is everything else. `spawn_daemon` binds the port and,
on `EADDRINUSE`, reuses whatever is already there with no check of the RMW, the
environment, or the discovery configuration. The daemon's own
`--rmw-implementation` / `--ros-domain-id` arguments are asserted once at
STARTUP, by the daemon about itself, and never consulted again by a caller.

So the leaked state is the **discovery configuration**: `CYCLONEDDS_URI`,
`FASTRTPS_DEFAULT_PROFILES_FILE`, `ZENOH_SESSION_CONFIG_URI`. The daemon's
participant is built from whichever process started it, and every later caller
on that domain gets a graph computed under those settings instead of its own.

## Measurement

Humble, `rmw_cyclonedds_cpp`, single host, `demo_nodes_cpp talker` as the live
node. The isolating profile is loopback-only, no multicast, empty peer list.

**False NEGATIVE** — daemon primed under the isolating `CYCLONEDDS_URI`, caller
unrestricted, both on domain 74:

```
2. Talker on domain 74 with DEFAULT config (fully reachable, definitely alive)
   [INFO] [talker]: Publishing: 'Hello World: 1'

3. ros2 node list, DEFAULT config caller, WITH daemon
   (empty)

4. ros2 node list --no-daemon, same caller
   /talker

5. ros2 param list /talker WITH daemon
   Node not found

6. ros2 param list /talker --no-daemon
   qos_overrides./parameter_events.publisher.depth
   ...
```

`Node not found` is the literal string the original investigation was reading.

**False POSITIVE** — the reverse priming, domain 73. Daemon started
unrestricted, caller carrying the isolating profile: the daemon reports
`/talker`, which the caller's own configuration cannot reach. Worth stating
because a remedy that only prevents empty results would leave this half
standing.

**Hypothesis that was disproved.** The first guess was an RMW mismatch — a
fastrtps daemon serving a cyclone caller. Measured on domain 91, it does not
reproduce: both are RTPS on the same domain and interoperate, so the daemon
sees the talker and answers correctly. Recorded because it is the plausible
wrong answer, and the class of wrong answer this repo has paid for before
(issues 0859–0862).

## Why nano-ros is MORE exposed than a single-domain user

Two facts compound:

1. **Issue 1009 / 1137.** This repo pins the interop bus with a profile FILE,
   per PROCESS, written into a tempdir that is deleted when the test ends. So
   the configuration a daemon captures is guaranteed to differ from the next
   caller's, and may name a path that no longer exists. A single-domain user
   with one static config never notices this leak, because there is nothing to
   leak.

2. **`unique_ros_domain_id()` RECYCLES domains**, and a daemon lingers for two
   hours after its last use (`ros2cli.daemon.serve`, `timeout=2*60*60`). So a
   later test landing on a domain some earlier test's daemon still holds is the
   expected case, not a rare one.

And the probe that was supposed to stop exactly that could not see it. Issue
0707's `domain_discovery_port_busy` reads the SPDP multicast port
`7400 + 250*d`. Measured, per RMW, with `ros2 daemon start` and nothing else
running:

| daemon's RMW | SPDP `7400+250*d` | daemon port `11511+d` |
| --- | --- | --- |
| `rmw_cyclonedds_cpp` | bound | listening |
| `rmw_fastrtps_cpp` | bound | listening |
| `rmw_zenoh_cpp` | **unbound** | listening |

A zenoh daemon is not an RTPS participant and binds no SPDP port at all. zenoh
is this project's default RMW, so the blind spot coincided exactly with the
common case: the allocator read the domain as free and handed out a domain that
already carried a foreign daemon.

## Remedy

Three parts, because no single one covers the class.

1. **One construction point.** `nros_tests::ros2::ros2_query_cmd(env_setup,
   secs, args)` appends `--no-daemon`, so the flag is decided once instead of
   remembered per site. It had been remembered 14 times and forgotten in
   `service_present_on_domain` — which is the shape CLAUDE.md's "fix the CLASS"
   rule describes: a rule enforced by copy-paste holds until the first copy that
   skips a line. That site is the issue-0741 precondition probe, so it was
   answering "is a foreign service already on this domain?" through the very
   mechanism that can invent one.

2. **The allocator learns the second port.** `domain_daemon_port_busy` reads
   `11511+d` from `/proc/net/tcp`, and `domain_busy` is the disjunction the
   allocator now consults. This is what defends the verbs that CANNOT take the
   flag (below), and it closes the zenoh blind spot in the table above.

3. **A gate.** `check-ros2-daemon-queries` (`scripts/check-ros2-daemon-queries.py`,
   fast line) refuses a graph-verb `ros2` command constructed without the flag.

### `ros2 action list` cannot comply

Measured on Humble: `ros2 action list --no-daemon` answers `error: unrecognized
arguments: --no-daemon`, and its `-h` never mentions the flag. Every other verb
this repo calls accepts it in trailing position — `node {list,info}`,
`topic {list,info}`, `topic info --verbose`, `service list`,
`param {list,get,set,describe}`, `lifecycle nodes`.

So `ros2_action_e2e.rs` is allowlisted and defended by remedy 2 instead. Passing
the flag there is not a stricter choice, it is a usage error that polls for 20 s
and then reports a DISCOVERY timeout — a failure mode that already cost a full
box run reading as an actions defect.

### Why not `ros2 daemon stop`

Issue 0763 settled this: under a parallel suite it is a cross-test kill, because
the daemon is a singleton and stopping it kills the one another test is mid-query
against. The rule stays "do not consult a shared daemon", never "restart the
shared daemon".

## Sweep

```
python3 scripts/check-ros2-daemon-queries.py     # the gate, over all tracked source
```

Before the fix this found 6 real sites (1 in `ros2.rs`, 5 in
`scripts/test/isotp-ros-params.sh`) plus the one that cannot comply. The
`isotp-ros-params.sh` five are notable for being a hand-run diagnostic — i.e.
exactly the by-hand debugging situation the original investigation was in — and
its `ros2 daemon stop` ran AFTER all five queries, where it could not have
helped even on 0763's since-rejected theory.

## Acceptance

* `nros_tests` unit: `a_bound_daemon_port_makes_the_domain_busy` binds
  `127.0.0.1:11511+d` directly, so the reproduction is deterministic and needs no
  ROS 2 install. It asserts the negative control first (the probe reads that
  domain FREE a moment earlier), because a probe stuck at `true` would pass the
  positive half alone — the failure shape issue 1043 records.
* `check-ros2-daemon-queries --self-test`, and the gate verified against a
  reintroduced violation on the real path (caught at `ros2.rs:940`, clean after
  restore).
