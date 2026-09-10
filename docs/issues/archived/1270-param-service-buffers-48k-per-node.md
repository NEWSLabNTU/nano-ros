---
id: 1270
title: "Parameter services cost 48 KiB of buffers per node, and nothing shares
  them, sizes them, or counts their RMW slots"
status: resolved
type: enhancement
area: core, rmw
severity: medium
resolved_in: "fix(#1270, #1271): one buffer pair per executor, six servers per node counted, and a dropped request says why"
related: [issue-1268, issue-1271]
---

## Resolution

Two of the three items in the fix shape are done. The third, sizing the
buffer from the declared parameters, is phase-446 W4 and goes through the
seam this change left for it.

- **Shared.** `ParamServiceBuffers` holds one request and one reply buffer
  per executor, allocated with the first set of services. Each of a node's
  six servers holds a backend handle and nothing else
  (`ParamServiceHandle`). The size goes through one function,
  `param_service_buffer_bytes()`, which returns `PARAM_SERVICE_BUFFER_SIZE`
  today.
- **Counted.** `EntityInventory` reads `param_services` / `lifecycle` from the
  model's `execution.features` (`InfraServices::from_model`, the predicate
  `nros ws entity-facts` now uses too). It adds 6 per node and 5 per
  executor to `max_queryables`, the number that reaches
  `NROS_DERIVED_MAX_QUERYABLES`, then Zephyr's `NROS_MAX_QUERYABLES` ->
  `ZPICO_MAX_QUERYABLES`, and `NROS_XRCE_MAX_SERVICE_SERVERS`. `MAX_CBS` is
  unchanged: both families live outside the arena. The inventory's mirrors
  of the counts are now held by `check-infra-queryable-counts`.

Measured on the host build (`size_of`, mock backend, default 4096):

| | before | after |
| --- | --- | --- |
| one node's boxed set of six | 63,992 B | 14,976 B |
| of which buffers | 49,152 B | 0 |
| of which the six mock handles | 14,832 B | 14,832 B |
| shared pair, per executor | -- | 8,192 B |
| four-node image, sets + pair | 255,968 B | 68,096 B |

The handle term is the MOCK backend's (2,472 B each, most of it the mock's
own 256-byte request slot and reply ring). A real image pays its backend's
`RmwServiceServer` instead, which is still not measured. The 144 B left per
node after the handles are the node key, the report mask and the node FQN
(128 B), which a dropped request is now reported against.

The cargo-leaf road still cannot derive `ZPICO_MAX_QUERYABLES`. Its
inventory comes from `nros-metadata.json`, which carries no bringup
features. See `leaf_entity_env.rs`.

## What an image pays

Declaring `param_services` gives every node six service servers (get, set,
set_atomically, list, describe, get_types), built by
`build_parameter_service_set` (`nros-node/src/executor/spin.rs`). Each is an
`EmbeddedServiceServer` with a request and a reply buffer of
`PARAM_SERVICE_BUFFER_SIZE` (4,096 B by default, `nros-node/build.rs`), boxed on
the heap:

| | per node | 4-node image |
| --- | --- | --- |
| buffers, 6 x (4,096 + 4,096) | 49,152 B | 196,608 B |
| Cyclone entities | 6 x (2 topics + 1 reader + 1 writer) | 24 readers, 24 writers, 48 topics |
| zenoh queryables | 6 | 24 |

The parameter store comes on top: one per executor, 285,440 B at the default
limits.

For scale: the downstream this was measured on (Autoware Safety Island,
MR-CANHUBK344) has a 94,208 B nros heap. The service buffers alone exceed it,
even if the store were sized to the image's 25 scalar parameters (4,200 B).

## Why it is larger than it needs to be

- **The buffers are not shared.** All six services on all nodes are polled
  from one spin, outside the executor arena, and each request is fully
  handled before the next is read. One request/reply pair per executor would
  serve them; today there are six per node.
- **The buffer size is not derived.** 4,096 B is a constant; the largest
  reply a node can produce (describe of all its parameters) follows from what
  it declares.
- **The RMW slots are not counted.** The entity inventory deliberately leaves
  them out ("NOT included: the parameter (6) and lifecycle (5) service
  families, which a feature enables and this inventory cannot see. An image
  carrying them must state the knob", `nros-cli-core/src/entity_inventory.rs`).
  So an image that declares `param_services` gets its queryable and service
  pools sized as if it had not.

## Fix shape

- Share one request/reply buffer across the parameter services of an
  executor.
- Size it from the image's declared parameters (the worst reply), with the
  current constant as the fallback when those are unknown.
- Count six service servers per node in the entity inventory whenever the
  bringup declares `param_services`.

## Not measured

`sizeof(RmwServiceServer)` per backend, and the zenoh `SERVICE_BUFFER` slot
each queryable holds.
