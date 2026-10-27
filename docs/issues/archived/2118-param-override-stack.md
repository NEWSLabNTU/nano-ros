---
id: 2118
title: "An argv parameter override held two ~8 KiB values per frame: one in the `Option` `typed_param_override` returned, one in the caller's binding"
status: resolved
type: limitation
area: [params, cpp, executor, memory]
severity: low
found: 2026-10-28
related: [2104, 2070, 2057]
resolved: 2026-10-28
resolved_in: "#2786"
---

## What

Issue 2104 took most parameter set-path frames down to one `ParameterValue`
(about 8 KiB), but left two frames at two values. On the Cortex-M3
velocity-smoother image (`mps2-an385`, FreeRTOS), the frame sizes (`sub sp`)
were:

| frame | bytes |
| --- | --- |
| `nros_cpp_node_list_param_overrides` | 17,280 |
| `Executor::declare_parameter_overridden` | 17,024 |
| `typed_param_override` (its own frame) | 8,576 |

Both are on the argv-override path, `-p name:=value` / `--params-file`.

## Cause

`typed_param_override` RETURNED an `Option<ParameterValue>`, so each caller
held two values:

- the `Option` the callee built in the caller's return slot;
- the binding the caller moved it into (`if let Some(v)` / `Some(value) =>`).

Issue 2104's measurement had named the cause wrongly. It blamed the declared
value plus the override, and the override plus its text buffer. The declared
value is borrowed, and the text buffer is 256 bytes, so neither is a second
value.

## Fix

`typed_param_override_into(text, declared, &mut out) -> bool` decides the
type, then writes the value into a slot the caller holds. Scalars are
assigned. A string goes through `ParameterValue::assign_string` (issue 2104),
which fills the existing buffer.

- `Executor::argv_param_override` takes the same `&mut out` and returns
  whether an override applied.
- `declare_parameter_overridden` and `declare_described_overridden` each hold
  ONE value and pass it, or the borrowed declaration, to the store.
- `for_each_argv_param_override_on` refills one value per override.
- `typed_param_override` is kept for the iterator `argv_param_overrides_on`,
  which hands values out by value. It is a two-line wrapper over the `_into`
  form, so the typing rule has one implementation.

## Measured

Same image, rebuilt:

| frame | before | after |
| --- | --- | --- |
| `nros_cpp_node_list_param_overrides` | 17,280 | 8,960 |
| `Executor::declare_parameter_overridden` | 17,024 | 8,768 |
| `typed_param_override` | 8,576 | gone (inlined, no value of its own) |

One value is the floor while a value has inline storage.

**Tests:**

- nros-node lib tests pass (747).
- `just check cpp` passes, including the `parameter_value_runtime` override
  cases.
- The node boots on QEMU and activates. Its peak (16,800 of 65,536) is
  unchanged, because it passes no argv overrides, so these frames never run
  there.
