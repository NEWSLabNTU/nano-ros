---
id: 2070
title: "Declaring one parameter costs ~67 KiB of task stack at the default capacities; the C++ FreeRTOS carrier's 64 KiB app stack overflows silently into kernel globals"
status: resolved
type: bug
area: [params, freertos, cpp, sizing]
severity: high
found: 2026-10-26
related: [2043, 1146, phase-518, phase-488]
resolved: 2026-10-27
resolved_in: "#2725"
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
Its hint to raise `[node.rt] app_stack_bytes` also did not apply on this road:
the carrier's value was a literal (see direction 3).

## Workaround

Set the capacities this image actually needs. For a node that declares only
scalars and strings:

```
NROS_MAX_STRING_VALUE_LEN=64 NROS_MAX_ARRAY_LEN=4 NROS_MAX_BYTE_ARRAY_LEN=64
```

## Node 2 (phase-518 W5)

Nav2's `nav2_velocity_smoother` declares 13 parameters, some of them double
arrays. Built with the same reduced capacities as node 1 (`64 / 4 / 64`), its
FreeRTOS boot report reads `app task stack peak 100416 of 196608 bytes`: a
64 KiB stack is not enough even after the capacities are cut. At 64 KiB the
image printed `*** STACK OVERFLOW: app ***` right after creating its lifecycle
services.

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

   **The knob half landed (phase-518).** The C/C++ carrier's
   `.app_stack_bytes` now comes from `NROS_FREERTOS_APP_STACK_KB` set in the
   configure (default 64), the cmake spelling of the cargo road's knob of the
   same name, and the boot hint names it. The early warning has not landed,
   and neither has directions 1 or 2, so this stays open: at the default
   capacities a ported node that declares a parameter still needs more stack
   than the default gives it.

## Resolution

Direction 2, plus a second instance of the same class that node 2 exposed.

**The declare path borrows the value.** `Executor::declare_parameter_on_ref`,
`ParameterServer::declare_ref` / `try_declare_ref` and
`ParameterValue::assign_from` carry a `&ParameterValue` down to the table slot.
The slot is written in place with `NotSet`, then the value is copied into it
once. The by-value spellings forward to them. The argv-override branch,
which has to own a typed copy, runs in its own `#[inline(never)]` frame, so the
common path carries no second value. Each `nros_cpp_node_declare_param_*`
builds one value and lends it to both the census hook and the store.

**Lifecycle registration builds its servers in their allocation.**
`register_lifecycle_services_named` built the five REP-2002 servers by value,
with ten receive buffers of `PARAM_SERVICE_BUFFER_SIZE` each, and then boxed
them. Its frame was 92,160 bytes on Cortex-M3 for node 2, whose buffers derive
to 9,216 bytes. That frame, not the declare path, was node 2's 100 KiB.
`LifecycleServiceServers::try_new_boxed` now initialises them through
`try_box_uninit`, the pattern `leak_parameter_storage` already uses, and a
failed allocation is a `BadAlloc` refusal.

Directions 1 and 3's early warning were not needed for this and were not taken.
Direction 1 is unsound for a lifecycle node anyway: Nav2 declares its
parameters in `on_configure`, which a census never reaches.

## Measured

Frame sizes for one `declare_parameter<double>`, aarch64, same sources
(phase-488 node 1 host build, bytes). The table is bounded by each symbol's
`nm -S` size:

| frame | before | after |
| --- | --- | --- |
| `nros_cpp_node_declare_param_double` | 8,608 | 8,640 |
| `Executor::declare_parameter_on` | 17,024 | 32 (forwards) |
| `Executor::declare_parameter_on_ref` | -- | 80 |
| `apply_argv_param_override` | 8,928 | override branch only |
| `ParameterServer::try_declare_with_descriptor` | 5,920 | 32 (forwards) |
| `Parameter::new` | 8,656 | not on the path |
| `ParameterServer::declare_ref` + `try_declare_ref` | -- | 416 + 672 |
| `ParameterValue::assign_from` | -- | 624 |

On Cortex-M3 (QEMU mps2-an385, FreeRTOS, lwIP, stock ROS 2 Humble peer),
both phase-518 nodes ran at the DEFAULT capacities (string 256, array 32) and
the DEFAULT 64 KiB app stack:

- node 1 (`autoware_external_cmd_selector`): `app task stack peak 12504 of
  65536 bytes`, in `ros2 node list`, publishes its mode, and answers
  `select_external_command` (`success=True`, mode 1 -> 2). Before, the same
  image overflowed into the kernel globals.
- node 2 (`nav2_velocity_smoother`): `app task stack peak 16800 of 65536
  bytes`, configures and activates through its lifecycle services, and
  publishes smoothed velocity. Before: a 101,352-byte peak, and `*** STACK
  OVERFLOW: app ***` at 64 KiB.
