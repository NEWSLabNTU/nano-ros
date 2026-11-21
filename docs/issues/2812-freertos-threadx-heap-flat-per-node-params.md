---
id: 2812
title: "The FreeRTOS heap and the ThreadX byte pool price the parameter services flat, on images that serve them on one node"
status: open
type: limitation
area: [freertos, threadx, memory, sizing, parameters]
severity: low
found: 2026-11-21
related: [2780]
---

## What is wrong

Every node that serves the parameter services runs six servers. Each server
owns a request/reply buffer pair of `PARAM_SERVICE_BUFFER_SIZE` bytes on the
platform heap: 12 x 4,096 = 49,152 bytes a node at the default. Issue 2780 hit
this on Zephyr. The flat 112 KiB arm had been priced on a one-node image, so a
four-node image ran the heap dry at boot. Its fix derives the Zephyr floor from
the entity inventory's `NROS_ENTITY_PARAM_SERVICE_NODES`.

The two other ports size their heap from a FLAT working set, measured on the
in-tree images. Neither has a per-node term.

| port | heap | how it is sized | room above the worst measured peak |
| --- | --- | --- | --- |
| FreeRTOS (zenoh) | heap_4, `NROS_FREERTOS_HEAP_KB` | `default_heap_bytes` (`nros-board-common/src/freertos_config.rs`): 3 x app stack + 2 x 131,072 spare executor + 65,536 working set = 720,896 at the 128 KiB stack | 331,832 above `realtime-rust`'s 389,064, about 6.7 extra parameter-serving nodes |
| ThreadX (zenoh) | byte pool, `NROS_THREADX_BYTE_POOL_KB` | `threadx_pool.rs`: app stack + `ZENOH_WORKING_SET_BYTES` 327,680 | 126,880 above the measured 200,800 working set, about 2.6 extra nodes |

So neither is broken by any image in the tree today. It is the same shape as
2780 before that image existed: the margin is what absorbs extra nodes, and
nothing says how many it can absorb. A ThreadX zenoh image serving the
parameter services on four nodes needs about 147,456 bytes beyond the one-node
working set. That is more than the pool's margin, and on this port the pool
also hands out the task stacks. So the failure would be an allocation failure
at boot, as on Zephyr.

## Fix direction

Give each derivation the same per-node term the Zephyr floor uses, from the
same count. The cargo road reads it through the sizing descriptor (a
`param_service_nodes` fact, stated from `InfraServices::param_nodes`, beside
`lifecycle_nodes`). The cmake road reads it from the entity-inventory fragment.
Measure the per-node remainder on each port, as 2780 does on Zephyr, rather
than assuming 12 buffers is the whole cost.

## Closes when

A multi-node parameter-serving image on each port boots with its derived heap.
A one-node image's heap is unchanged.
