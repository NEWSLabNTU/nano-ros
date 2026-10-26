---
id: 2070
title: "Declaring one parameter costs ~67 KiB of task stack at the default capacities; the C++ FreeRTOS carrier's 64 KiB app stack overflows silently into kernel globals"
status: open
type: bug
area: [params, freertos, cpp, sizing]
severity: high
found: 2026-10-26
related: [2043, 1146, phase-518, phase-488]
---

## What

`ParameterValue` stores every variant inline, so its size is the size of its
largest variant. At the default capacities (`NROS_MAX_ARRAY_LEN` 32,
`NROS_MAX_STRING_VALUE_LEN` 256), `StringArray` is 32 × 256 bytes, so every
`ParameterValue` takes about 8 KiB, even a `Double`.

The declare path passes the value by value through five frames, and each frame
keeps its own copy or temporary. On `arm-none-eabi` (Cortex-M3), one call to
`declare_parameter<double>` used this much stack. The Rust frames measured the
same with `CMAKE_BUILD_TYPE` empty and with `Release` (issue 2072):

| frame | stack |
| --- | --- |
| `nros_cpp_node_declare_param_double` | 8.4 KiB |
| `Executor::declare_parameter_on` | 16.7 KiB |
| `ParameterServer::try_declare_with_descriptor` | 33.9 KiB |
| `Parameter::new` | 8.5 KiB |
| **declare path total** | **~67 KiB** |

The node's own constructor frame was another 6.8 KiB unoptimised and 2.1 KiB
under `Release`. In the `Release` image the app stack's top was `0x2015c878`
and the overflowing write landed at `0x2014b588`, 70,384 bytes down.

With the capacities below, the same image's boot report reads
`app task stack peak 13768 of 65536 bytes`.

The C++ FreeRTOS typed carrier (`cmake/templates/freertos_app_config.c.in`)
hardcodes `.app_stack_bytes = 65536u`. Issue 1146 measured that figure on the
in-tree images, and none of them declares a parameter. The app task's stack is
a heap_4 block, so an overflow runs below `ucHeap` into the FreeRTOS kernel
globals. FreeRTOS's stack-overflow check does not catch it.

## How it showed

Phase 518 node 1 is `autoware_external_cmd_selector`, a stock Autoware
component that declares two parameters. Its FreeRTOS image on QEMU mps2-an385
showed up in `ros2 node list` and then stopped with:

```
FreeRTOS ASSERT FAILED: .../freertos-kernel/10.6.2+0adc196d/queue.c:1678
```

That assert means a blocking take while the scheduler is suspended. The
scheduler was not suspended. `uxSchedulerSuspended` held the ASCII bytes
`"a48/"`, the tail of a zenoh liveliness keyexpr. A hardware watchpoint on
`xQueueRegistry` caught the write in `Parameter::new`'s `memcpy`, with SP at
`0x20148350` (inside lwIP's `memp` pools). The app stack's top was `0x2015a908`
and the block was 64 KiB.

The board's `report_app_stack_peak` never prints, because the image dies first.
Its hint to raise `[node.rt] app_stack_bytes` also does not apply on this road:
the carrier's value is a literal.

## Workaround

Set the capacities this image actually needs. For a node that declares only
scalars and strings:

```
NROS_MAX_STRING_VALUE_LEN=64 NROS_MAX_ARRAY_LEN=4 NROS_MAX_BYTE_ARRAY_LEN=64
```

## Fix directions

1. **Derive the capacities from what the image declares (issue 2043's class).**
   `nros-params/build.rs` already reads a NEED per capacity from the sizing
   descriptor. A ported image that declares no array parameter could take
   capacity 0 for the array variants, and that shrinks every copy. The cmake
   RTOS road carries no census today, which is 2043.
2. **Stop copying the value through the declare path.** Construct the entry in
   its table slot and move the caller's value in once, so the cost is one
   value instead of five. This fixes the class whatever the capacities are.
3. **Make the C++ FreeRTOS app stack a stated knob,** not a literal, and print
   the peak the moment it crosses a fraction of the budget, so an overflow
   says so before it corrupts the kernel.
