---
id: 2812
title: "The FreeRTOS heap and the ThreadX byte pool price the parameter services flat, on images that serve them on one node"
status: resolved
type: limitation
area: [freertos, threadx, memory, sizing, parameters]
severity: low
found: 2026-11-21
related: [2780]
resolved: 2026-11-22
resolved_in: "the change that archives this issue"
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
| ThreadX (zenoh) | byte pool, `NROS_THREADX_BYTE_POOL_BYTES` | `threadx_pool.rs`: app stack + `ZENOH_WORKING_SET_BYTES` 327,680 | 126,880 above the measured 200,800 working set, about 2.6 extra nodes |

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

## Resolution (2026-11-22)

Both ports' derived defaults now add the per-node term, on both roads, from
one formula. The arithmetic is `cmake/NanoRosServiceHeap.cmake` and its Rust
twin `nros_sizing_descriptor::service_heap`; a test reads the cmake constants
so the two cannot drift.

    extra = (param_nodes - 1) x (6 x 2 x buffer + param_remainder)
          + (machines - 1)    x (5 x 2 x buffer + lifecycle_remainder)

- **Cargo road.** The sizing descriptor states `[image] param_service_nodes`,
  from the same count the queryable pool uses (`InfraServices::param_nodes`);
  lifecycle machines reuse `infra::lifecycle_machines`. The ThreadX pool
  (`threadx_sources.rs`) and the FreeRTOS heap (`nros-board-freertos/build.rs`)
  add the extra to their derived defaults, and the ThreadX floor a stated pool
  is checked against rises by the same amount.
- **cmake road.** `nano_ros_entry` reads the image's entity-inventory fragment
  and defines `NROS_FREERTOS_SERVICE_HEAP_EXTRA_KB` on `freertos_config` or
  `NROS_THREADX_SERVICE_HEAP_EXTRA_BYTES` on `threadx_glue`
  (`cmake/NanoRosRtosServiceHeap.cmake`). Nothing is defined when the extra is
  0, and both C defaults read 0 when undefined.
- **Buffer.** Both roads price nros-node's default 4,096-byte buffer.
  `NROS_EXECUTOR_PARAM_SERVICE_BUFFER_BYTES` has one sanctioned reader (nros-node), so a
  stated smaller buffer over-provisions by the difference.

**Remainders: 0 on both ports, measured.** On rv-virt-threadx each parameter
node past the first cost 34,024 bytes of pool (peak 742,240 with one node,
844,312 with four), under the 49,152 its buffers are priced at, so there is no
remainder to add and the term over-provisions by about 15 KB a node. FreeRTOS
was not measured to that precision: its heap keeps hundreds of KiB of headroom
at five nodes, so its remainder is 0 by the same reading.

**Acceptance, five parameter-serving nodes** (`tests/param_service_heap.rs`,
rows `threadx-riscv64-param-five-node` and `freertos-param-five-node`). Four
nodes still fit the flat ThreadX pool, with about 7.6 KB to spare, so the
fixture is five.

| image | flat (main) | derived |
| --- | --- | --- |
| rv-virt-threadx byte pool | 851,968 B, peak 851,880 to 851,968 (**0 to 88 free** across three boots): `heap exhausted` for a service server handle, `/param_e`'s parameter services not created. FAILS | 1,048,576 B, peak 878,504, 170,072 free. PASSES |
| mps2-an385-freertos heap_4 | 720,896 B, peak 285,568, 435,328 free | 917,504 B, peak 285,544, 631,960 free |

Both grew by 4 x 49,152 = 196,608 bytes. The ThreadX image on the flat pool
still delivered on `/chatter`, which is why the test also asserts that the
peak line leaves bytes free and that no `heap exhausted` line appears. Each
catches a boot the other misses: one flat boot ended with 88 bytes free and a
refused service server.

**One node is unchanged.** `threadx-riscv64-param-store` boots with the same
851,968-byte pool as before, and an image with one node and one machine defines
nothing on either road (`just check rtos-service-heap` runs the cmake function
for both cases).
