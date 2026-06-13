---
id: 53
title: ParameterServer get_parameter<int> fails — get_impl(int&) overload missing (declare/set have it)
status: resolved
type: bug
area: c-api
related: [phase-242, rfc-0044]
resolved_in: ASI FVP workspace-mode bring-up (parameter.hpp — add get_impl(int&))
---

## Symptom

A rclcpp-faithful component (ASI MPC lateral controller) declaring `int`
parameters failed to compile:

```
nros/parameter.hpp:269: error: no matching function for call to
  'get_impl(const char*&, int&)'
…cannot bind non-const lvalue reference of type 'bool&'/'int64_t&'/'double&'
  to a value of type 'int'
```

## Root cause

The `ParameterServer` scalar facade (Phase 242) had asymmetric `int` support.
`declare_impl(const char*, int)` and `set_impl(const char*, int)` both exist
(they collapse `int` → `int64_t` via the integer slot), but the matching
`get_impl(const char*, int&)` was never added. `get_parameter<int>` therefore
had no viable overload — the `int64_t&`/`bool&`/`double&` candidates can't bind
an `int&`. rclcpp nodes routinely use plain `int` parameters, so the value-
returning `int ComponentNode::get_parameter<int>(...)` facade hit this.

## Fix

Add the symmetric reader: read through the `int64_t` slot, then narrow.

```cpp
Result get_impl(const char* name, int& out) const {
    int64_t v = 0;
    Result r(nros_param_get_integer(&server_, name, &v));
    if (r.ok()) { out = static_cast<int>(v); }
    return r;
}
```

Verified: the full Autoware component library (which declares `int` params)
compiles for `aarch64-zephyr-elf`.
