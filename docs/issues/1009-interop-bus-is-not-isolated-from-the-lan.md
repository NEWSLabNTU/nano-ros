---
id: 1009
title: "Our DDS interop tests share a bus with the whole LAN, so a foreign peer
  on another host can fail them — and `ROS_LOCALHOST_ONLY=1` alone does NOT fix it"
status: open
type: bug
area: testing, ci
severity: high
found: 2026-09-03
related: [issue-0741, issue-0707, issue-0927, phase-414]
---

## What happened

Issue 0741 spent fifteen sections and four wrong diagnoses on a 28-byte DDS
reply that Fast-DDS refused into a 15-byte reader history. The sample was not
ours. It came from **a CycloneDDS `add_two_ints_server` on a different host**
(`arm-a100`, 10.2.15.142; this host is 10.2.15.118), which is squatting **35 ROS
domains** — 1, 2, 3, 4, 5, 10, 12, 20, 27, 29, 31, 36, 40, 42, 46, 47, 49, 59,
65, 67, 68, 69, 70, 72, 81, 82, 83, 84, 85, 86, 89, 90, 93, 99 — one participant
each, all announcing `__ProcessName=add_two_ints_server`,
`__Hostname=arm-a100`, CycloneDDS 0.10.5.

**The repro 0741 never had, and it needs none of our code:**

    $ ROS_DOMAIN_ID=1 ros2 service call /add_two_ints \
        example_interfaces/srv/AddTwoInts "{a: 5, b: 3}"
    [RTPS_READER_HISTORY Error] Change payload size of '28' bytes is larger than
    the history payload size of '15' bytes and cannot be resized.

3 of 3 here, 5 of 5 in the original measurement. No nano-ros process, no XRCE
agent, nothing of ours running. Domain 9 is empty and is the control.

The 28 bytes are the **rmw_cyclonedds** request/reply mapping
(`[client GUID 8][seq 8][response 8]`): the Cyclone server read a Fast-DDS
request — which carries only the payload, with the identity in inline QoS
`PID 0x800f RELATED_SAMPLE_IDENTITY` — as its own header, giving
`guid=5, seq=3, sum=0`. The two service mappings are not interoperable. Bytes
measured; that interpretation inferred.

## Why this is OUR issue and not just someone else's stray process

Killing those 35 orphans fixes today. It does not fix the class: **our interop
tests discover on whatever multicast the host can reach**, so any DDS
participant on the LAN — a colleague's laptop, a robot, a CI runner — is a
peer. This has now cost one issue weeks of misdirected work, and the same shape
already appeared in issue 0927 (a probe alone on a domain enumerating itself)
and issue 0707 (stepping off a busy SPDP port).

**And the intermittency was never randomness.** `dds_bus_snapshot`
(`packages/testing/nros-tests/src/ros2.rs:2037`) runs `ros2 node list` /
`service list` / `topic list` WITHOUT `--no-daemon`, unlike its three siblings
at lines 762/775/948. Every failing run therefore leaves a ros2 daemon on that
domain; `domain_discovery_port_busy` then reads the port as busy and the next
run steps to the NEXT domain. After two failures the run lands on a domain with
no foreign peer and can never fail again. That is exactly the 1-fail-then-45-
green pattern 0741 recorded and could not explain.

## `ROS_LOCALHOST_ONLY=1` IS NOT THE FIX — measured

| batch | condition | domain | pass / fail |
| --- | --- | ---: | ---: |
| D | per-run daemon kill, UDP-only profile | 1 | 12 / **3** |
| F | `ROS_LOCALHOST_ONLY=1` alone | 1 | **0 / 15** |
| G | Fast-DDS `interfaceWhiteList 127.0.0.1`, BOTH processes | 1 | **15 / 15** |

`ROS_LOCALHOST_ONLY=1` alone gives 0 of 15 — empty client output, no history
error at all — because **the XRCE Agent is a bare Fast-DDS application, not a
ROS process**, so it ignores the variable and the two stop discovering each
other. A variable that isolates one side of a pair is worse than none: it turns
a wrong answer into no answer.

What is measured to work is a `FASTRTPS_DEFAULT_PROFILES_FILE` whose transport
declares `<interfaceWhiteList><address>127.0.0.1</address></interfaceWhiteList>`,
exported so BOTH the peer and the Agent use it: 15 of 15 on the poisoned domain.

Note also that a custom `userTransports` profile DEFEATS `ROS_LOCALHOST_ONLY`,
so the two must not be combined naively (batch E: 9/6, confounded).

Cyclone lanes need the `CYCLONEDDS_URI` equivalent; that is not yet written.

## Direction

1. **Isolate the interop bus to loopback, for every DDS lane and for the Agent
   too.** The profile route above is measured; the Cyclone equivalent is not.
   This is the fix that survives the LAN.
2. **`dds_bus_snapshot` must pass `--no-daemon`** like its siblings. Worth doing
   regardless — but note it could never have caught this anyway: `ros2 service
   list` collapses a service to one NAME however many servers offer it, and the
   helper's own comment pre-explains an empty `[nodes]` as normal, which is
   exactly where `/add_two_ints_server` would have appeared.
3. **`domain_discovery_port_busy` is local-only by design** — its doc-comment
   says so — and is therefore structurally blind to a REMOTE squatter. Worth
   stating in issue 0707's terms rather than changing: a local port probe cannot
   answer a question about the LAN.

## Acceptance

With the 35 foreign participants still live, the XRCE service interop cell
passes on a poisoned domain (1 or 5), repeatedly, with no per-run domain
walking.

## Not covered

The orphans themselves. `arm-a100` is a different machine and clearing it needs
access to that host; it is the direct cause of today's failures and none of the
fix above depends on it.
