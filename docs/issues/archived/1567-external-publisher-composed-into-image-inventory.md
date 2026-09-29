---
id: 1567
title: "A publisher a contract NAMES on an external topic was composed into the image's entity inventory, refused the transient-local count, and sized the queryable table short"
status: resolved
type: bug
area: [cli, sizing]
severity: high
found: 2026-09-29
related: [1378, 1393, 1549]
resolved_in: "branch fix/inventory-skips-external-endpoints"
---

## What

`EntityInventory::from_model` built one component per node FQN it found in
`structure.topics[*].{publishers,subscribers}`. A contract may name who
publishes an EXTERNAL topic -- the checker needs that publisher to price a
hazard's detection (its period plus its path latency) -- and the resolver keeps
that endpoint ref in the wiring beside the image's own, marking the side in
`contracts.externals`. The inventory read the wiring and not the mark, so the
named node became a component of the image.

## Measured

Autoware Safety Island (Zephyr 4.4, west, `mps2/an385` under QEMU), island
main 9574cd0 on nano-ros a259c7058. The contract states

```yaml
/system/operation_mode/availability:
  type: tier4_system_msgs/msg/OperationModeAvailability
  external: pub
  pub: [/availability_gate/availability]
```

and the build printed

```
-- nros: entity inventory DERIVED from 5 components -- 35 entities, ...
```

for an image of four nodes. The fifth row, `/availability_gate`, states no
durability (nobody writes QoS for a node they do not build), so:

```
set(NROS_DERIVED_MAX_QUERYABLES 2)
# NROS_DERIVED_TL_PUBLISHERS is not derived: publisher /system/operation_mode/availability (...) states no `durability` ...
```

The image carries two service servers and five TRANSIENT_LOCAL publishers,
each of which declares a cache queryable (issue 1378). A refused count
contributes zero, so the table held the two services only, and the first
latched publisher failed at boot:

```
[ERROR] .../nros/node.hpp:332 node "mrm_comfortable_stop_operator": FAILED at create_publisher_in (code=-3)
stage      4  RegisteringEntities -- an entity claimed arena; registration in flight
```

`-3` is `InvalidArgument`: `zpico` `Full` -> `TransportError::InvalidConfig`
-> `NROS_CPP_RET_INVALID_ARGUMENT`.

## Fix

`from_model` skips an endpoint when BOTH hold: the topic's `externals` side
covers the endpoint's role, and the endpoint's node is not a key of
`structure.nodes`. Either test alone is unsafe -- the node map's key is not
always the ROS name (a `<node>` with no `name=` is keyed by executable), and an
external mark names a side of a topic that an in-image node may also occupy.
Tests: `a_named_external_publisher_is_not_a_component_of_the_image`,
`an_external_mark_keeps_an_in_image_endpoint`.

## Not fixed here

A refused transient-local count still contributes ZERO cache queryables to
`MAX_QUERYABLES`, which under-sizes the table whenever a real in-image
publisher is silent about durability. That direction is the unsafe one for a
pool; it is left for its own change because it moves every image with a
silent publisher.
