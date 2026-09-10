---
id: 1291
title: "Two Cyclone clients of one service in one process share a reply id, so
  each accepts the other's replies as its own"
status: open
type: bug
area: rmw
severity: high
related: [issue-1088, issue-0778]
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
