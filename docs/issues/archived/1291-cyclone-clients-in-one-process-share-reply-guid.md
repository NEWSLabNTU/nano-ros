---
id: 1291
title: "Two Cyclone clients of one service in one process share a reply id, so
  each accepts the other's replies as its own"
status: resolved
type: bug
area: rmw
severity: high
related: [issue-1088, issue-0778, phase-480]
---

## Symptom

Found by the issue-1088 regression test (`service_request_slots_exhausted`),
which runs five clients of one service on one participant. With the correct
server, the reply check failed:

```
reply 7 sum 1000, want 1056
```

Client 1 took the reply to client 0's request 0 and reported it as the answer
to its own request 7. The sequence id matched, the payload was another
client's, and the call returned success.

## Cause

`nros-rmw-cyclonedds/src/service.cpp` puts a 64-bit client id in the request
header (`cdds_request_header_t.guid`). The server echoes it, and
`take_response` keeps a reply only if `got_id.guid == state->my_guid` and the
sequence number is outstanding on this client.

`my_guid` came from `writer_guid_lo64`, which did `memcpy(&v, g.v, 8)` on the
request writer's RTPS GUID. Those are the first 8 bytes, not the "lower 8" its
comment claimed. They are the GUID **prefix**: host id and app id. Every writer
in one process has the same prefix, so every client in one process had the
same `my_guid`. Each client numbers its requests from the same start, so two
clients of the same service in one image collide on every sequence number
they share. The one filter that separates them compares two equal values.

Upstream `rmw_cyclonedds_cpp` puts the request writer's **instance handle**
(`pubiid`) in that field. The instance handle is unique per entity within a
process. The server only echoes the value, so any per-client-unique value is
wire-compatible.

## Reach

- Only the same service. Different services use different reply topics.
- Two clients of one service in one process: two nodes composed onto one
  executor that call the same service, or one node with two clients of it.
- Silent. The wrong reply is delivered with `taken = true` and OK, and the
  right reply is then dropped as "not ours" — or taken by the other client,
  which made the same mistake.

## Fix

`writer_request_id64`: the request writer's `dds_get_instance_handle`, with
the existing random fallback when that call fails. Landed with the
issue-1088 change in the same pull request. `service_request_slots_exhausted`
is the regression test. Negative control: with the GUID-prefix id put back,
the test fails `rc=18` with the line quoted above. With the instance handle,
all 35 replies reach the client that asked.

## Not covered

- Only this test runs two clients of one service in one process, and it
  checks one participant. It does not check two participants in one process.
  The instance handle is process-unique, so that case is covered by
  construction, not by measurement.
- Interop with a stock ROS 2 server was not re-run. The server echoes the
  field it received, and the value is only ever compared by the client that
  wrote it, so the wire contract is unchanged.

## Resolution

Resolved 2026-10-06 (phase-480 W4). The fix itself landed with issue 1088
([1088-cyclone-take-request-destroys-and-reports-empty.md](1088-cyclone-take-request-destroys-and-reports-empty.md)):
`writer_request_id64` puts the request writer's instance handle in the request
header. This closes the second "Not covered" item above, the stock-server run.

**The test.** `ros2_srv_client` gained a pair mode (`NROS_SRV_CLIENT_PAIR=1`):
two clients of `/add_two_ints` on one node of one process. Each round sends both
requests before either client takes, so both are outstanding at once with equal
sequence numbers. The payloads differ per client (`b = 100` and `b = 1000`), so
a reply crossed between clients is a wrong sum, not a lucky match.
`ros2_srv_e2e.sh` runs it as the third sub-case of
`nros_rmw_cyclonedds_ros2_srv_e2e`, against the same stock
`demo_nodes_cpp add_two_ints_server` (humble `rmw_cyclonedds_cpp`, bus pinned
to loopback) that sub-case B.2 starts.

**Measured:**

| tree | result |
| --- | --- |
| with the fix | `PAIR_OK rounds=5`, 3 of 3 runs |
| `writer_request_id64` put back to the GUID prefix (`memcpy` of the first 8 GUID bytes) | `PAIR_MISMATCH client=1 round=0 sum=100 want=1000` |

So the stock server echoes the instance handle unchanged, and the client-side
filter separates the two clients. The wire contract held, as the issue argued.

**Not measured:**

- Two participants in one process. That is still covered by construction (the
  instance handle is unique per process), not by a run.
- A stock `rclpy` server, and Jazzy. Only humble `rclcpp` was run.
