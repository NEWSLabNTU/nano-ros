---
id: 2104
title: "The parameter set paths copied an ~8 KiB `ParameterValue` through every frame, so a C++ `set_param_*` cost two or three values of stack"
status: resolved
type: limitation
area: [params, cpp, executor, memory]
severity: medium
found: 2026-10-27
related: [2098, 1146]
resolved: 2026-10-27
resolved_in: "the PR that archives this file"
---

## What

`ParameterValue` is about 8 KiB: its string and array arms are inline
fixed-capacity buffers. The set paths took it BY VALUE at every layer, from
the `nros-cpp` shim through `Executor::set_parameter_on` and
`ParameterServer::apply` to `set`. Each layer's frame held its own copy, and
`Option<ParameterValue>` temporaries added another. On the Cortex-M3
velocity-smoother image (node 2, issue 2098's board), `objdump` frame sizes
(`sub sp, #N`) were:

| frame | before |
| --- | --- |
| `nros_cpp_node_set_params_atomically` | 24,960 |
| its `Map<Iter<param_write_t>>::next` closure | 17,152 |
| `nros_cpp_node_list_param_overrides` | 17,280 |
| `Executor::declare_parameter_overridden` | 25,536 |
| `ParameterServer::set_parameter_range` | 16,896 |
| `nros_cpp_node_set_param_*_array` | ~16,600 |

The atomic set nested its closure inside its own frame, so a single call
needed over 40 KiB of stack on a board whose default app stack is 64 KiB.

## Fix

The set path borrows. `ParameterServer::apply_ref` / `set_ref` take
`&ParameterValue` and write in place with `assign_from`;
`Executor::set_parameter_on_ref` forwards the borrow.
`set_parameter_range` checks the range through `as_ref()` instead of cloning
the stored value.

A multi-element set fills ONE scratch slot.
`ParameterServer::apply_atomically_with` /
`Executor::set_parameters_atomically_with_on` take a
`fill(index, &mut ParameterValue) -> Option<&str>` callback and run both
passes (validate, then commit) over that slot. The shim writes each element
straight into it: scalars by assignment, strings and arrays through the new
`ParameterValue::assign_string` / `assign_*_array`, which copy into the
existing buffers. The iterator form of `apply_atomically` is kept and now
adapts onto the callback.

`list_param_overrides` walks the overrides with
`Executor::for_each_argv_param_override_on` instead of collecting them.
`typed_param_override` decides the type first and builds the value once.

## Measured

Same image, rebuilt:

| frame | after |
| --- | --- |
| `nros_cpp_node_set_params_atomically` (+ closure) | below 4,000 |
| `ParameterServer::set_parameter_range` | below 4,000 |
| `nros_cpp_node_set_param_*_array` | 8,320–8,384 |
| `Executor::declare_parameter_overridden` | 17,024 |
| `nros_cpp_node_list_param_overrides` | 17,280 |
| `typed_param_override` (now its own frame) | 8,576 |

On QEMU the node reports `app task stack peak 16800 of 65536 bytes`, then
activates and publishes. `just check cpp` passes, including the
`parameter_value_runtime` atomic-set and override cases.

## Left as is

Each remaining frame of about 8 KiB holds exactly ONE value. That is the
floor while a value has inline storage, and moving it to the heap is a
design change, not a fix.

Two frames still hold two values. `declare_parameter_overridden` holds the
override and the declared value it falls back to. `list_param_overrides`
holds the override being reported plus its text buffer. Each is 17 KiB
measured and called once per declaration or listing, which fits the 64 KiB
default app stack with room left. Removing the second value would need the
override to be written into the store slot directly. That is a separate
change; reopen this issue if a board's stack budget makes it necessary.
