---
id: 2919
title: "A second managed node does not fit the Zephyr heap: each one boxes 44,200 bytes of service servers, and the heap default is a constant"
status: open
type: bug
severity: medium
area: [lifecycle, sizing, zephyr, xrce]
related: [phase-554, issue-2904, issue-1551, issue-2070]
found: 2026-11-24
---

## What happens

A Zephyr image on mps2/an385 with XRCE and two managed nodes stops at boot.
The C talker leaf (`examples/zephyr/c/talker`) was built with two talker
instances, both marked `<lifecycle_node>` in the launch file, with
`[lifecycle] autostart = "none"`. Booted with `nros run`, the console reads:

```text
nros: HEAP EXHAUSTED (TOO SMALL): request 44200 bytes, arena 197120 bytes, free 38696 bytes, largest free block 38696 bytes, caller 0x3abbd
<err> nros: nros_cpp_lifecycle_register_node: the lifecycle services of `talker` could not be created
<err> nros: NodeError::Transport(BadAlloc): transport error: memory allocation failed
nros: heap peak 158320 of 197120 bytes (38800 free at the peak) - raise with [image.<id>.knobs.memory] heap_bytes
```

No node is visible: `ros2 node list` and `ros2 lifecycle nodes` are empty, and
`ros2 lifecycle get /talker` says `Node not found`. The same image with one of
the two nodes managed boots and publishes. So does the zenoh image with two
managed nodes, which has more heap free at that point, so it fits for now.

A user who adds a second `<lifecycle_node>` to an XRCE image on this board gets
an image that does nothing, and the size of the heap is the only thing that
decided it.

## Why

Two facts combine.

1. **Each managed node boxes ten generic service buffers on the heap.**
   `caller 0x3abbd` is
   `nros_node::lifecycle_services::LifecycleServiceServers::try_new_boxed`
   (`packages/core/nros-node/src/lifecycle_services.rs`). It boxes five
   servers, each with a request and a reply buffer of
   `LIFECYCLE_SERVICE_BUFFER_SIZE`, which is `PARAM_SERVICE_BUFFER_SIZE`
   (4,096 bytes here, the unstated default), plus the backend's server
   handle (648 bytes on XRCE): 5 x (2 x 4,096 + 648) = 44,200 bytes per
   managed node. Lifecycle
   requests and replies are small and fixed-shape. RFC-0128 D7 sizes the
   lifecycle inbox for them, not for the image's generic service size, but
   that is phase-554 W4/W5, which are still open.
2. **The Zephyr heap does not grow with the managed-node count.**
   `zephyr/Kconfig` sets `NROS_MEMORY_HEAP_BYTES` to `default 196608 if
   NROS_CAPABILITY_LIFECYCLE`, a constant. The sizing descriptor states
   `lifecycle_nodes = 2` for this image, but nothing reads it into the heap
   size. Its `heap_budget_bytes` is refused, because the board states no
   `[knobs.memory] heap_bytes`. The heap is the same for one managed node or
   two, so the second box fails.

The cost is also invisible to `just mem-report`, which reads symbols: the
images with zero, one and two managed nodes on XRCE have identical static
RAM (586,523 bytes), because the 44,200 bytes per node come out of the heap
arena, whose size does not change.

## Measured

Built on 2026-11-24 from main plus PR #4728, with scratch images in the talker bringup (two instances
of `nros_zephyr_talker_c::Talker`, 0, 1 or 2 of them `<lifecycle_node>`),
Zephyr 3.7.2, mps2/an385 under QEMU, `nros run`:

| RMW | managed nodes | boots | static RAM |
| --- | --- | --- | --- |
| XRCE | 0 | yes | 586,523 |
| XRCE | 1 | yes | 586,523 |
| XRCE | 2 | **no**, heap exhausted at the first `try_new_boxed` | 586,523 |
| zenoh | 2 | yes | 693,429 |

## Heap peaks

The last `nros: heap peak` line of each image (XRCE, 196,608-byte arm unless
stated):

| managed nodes | heap | result | peak |
| --- | --- | --- | --- |
| 0 | 196,608 | boots | 137,704 |
| 1 | 196,608 | boots | 183,312 |
| 2 | 196,608 | stops at the first machine | 158,320 |
| 2 | 245,760 | stops at the second machine (42,296 free) | 203,784 |
| 2 | 327,680 | boots | 250,896 |

The first machine adds 45,608 bytes and the second 67,584: its 40,960 bytes of
buffers, plus about 26 KB more on XRCE. The XRCE service-server cap is 19 with
zero or one managed node (the `[lifecycle]` block alone prices one machine) and
24 with two.

zenoh with two managed nodes boots on the same arm, peak 181,536.

## Fix

- **The heap derivation (part 2) LANDED** in PR #4569 (`a7080032e0`,
  2026-11-24). It derives the Zephyr heap from the parameter-serving node and
  lifecycle machine counts, with an XRCE lifecycle remainder measured from the
  peaks above. It adds a cell that boots two managed C nodes on XRCE on mps2
  with no heap stated (peak 251,120 of 320,000 B), so a second managed node no
  longer exhausts the heap at boot. Its count is
  `InfraServices::lifecycle_machines()`, which issue 2904's declared
  `[lifecycle] managed_nodes` feeds too.
- **The buffers (part 1)**: size the lifecycle servers' buffers from the
  lifecycle messages' own bounds, not the parameter service buffer
  (RFC-0128 D7's "inbox sized for lifecycle requests"). That is phase-554 W5.

With part 2 landed, what remains is part 1: each machine's 44,200 bytes is
an efficiency cost, not a boot failure. The nros12 release page drops this
issue's "two managed XRCE nodes, HEAP EXHAUSTED" known issue for that reason.

This closes when part 1 (phase-554 W5) lands too.

## Progress (2026-11-24): the buffers (phase-554 W5)

The second fact is #4569's, merged: the Zephyr heap now grows with the
machine count. This addresses the first.
- Each lifecycle server's request and reply buffers are sized for its own
  message (nros-node `lifecycle_services.rs`), no longer
  `PARAM_SERVICE_BUFFER_SIZE` each way. Measured worst-case replies, streamed
  in every state with the CDR header: get_state 28 B, get_available_states
  96, get_available_transitions 157, get_transition_graph 429.
- Buffers: replies 16 / 64 / 128 / 256 / 512 B; requests 512 B for
  `change_state` (a client sends any label) and 64 B for the four empty ones.
  That is 1,744 B per machine (`LIFECYCLE_MACHINE_BUFFER_BYTES`), down from
  40,960. On XRCE a machine's allocation is about 5 KB with its five 648-byte
  handles, down from 44,200.
- `lifecycle_buffers_hold_every_reply` streams each reply in every state and
  holds it under its buffer.
- The heap floor's per-machine term prices the new buffers, in both of its
  twins: `NanoRosServiceHeap.cmake` and
  `nros_sizing_descriptor::service_heap`. The service size no longer scales
  it, and a test holds both mirrors to nros-node's sum.
- The one-machine heap bases still price the old buffers (about 39 KB over).
  Re-measuring them is [issue 2953](2953-lifecycle-heap-arms-priced-old-buffers.md).

Left for acceptance: features' `zephyr_mps2_c_lifecycle_pair_xrce` still
boots, and its heap peak drops by about 2 x 39 KB from 251,120.

