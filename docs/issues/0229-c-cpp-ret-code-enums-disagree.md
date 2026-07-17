---
id: 229
title: "C nros_ret_t and C++ nros::ErrorCode disagree from -5 down — Result(C code) misreports ALREADY_EXISTS as Full"
status: open
type: bug
area: core
related: [phase-292, phase-269]
---

## Summary

`nros::Result` is routinely constructed straight from a C-ABI return code
(`Result(nros_param_declare_double(...))` and every other
`packages/core/nros-cpp/include/nros/parameter.hpp` shim), but the two enums
diverge from `-5` down:

| value | C `nros_ret_t` (nros_generated.h) | C++ `nros::ErrorCode` (result.hpp) |
| ----- | --------------------------------- | ---------------------------------- |
| -5    | `NROS_RET_ALREADY_EXISTS`         | `Full`                             |
| -6    | `NROS_RET_FULL`                   | `TryAgain`                         |
| -7    | `NROS_RET_NOT_INIT`               | (next…)                            |

Every `Result` built from a C code in that range reports the WRONG error to
C++ callers (`str()`, comparisons against `ErrorCode::…`, error routing).

## Impact

Phase-292 W2 (ASI FVP bring-up) burned a debugging round on exactly this: the
controller's `declare_parameter("wheel_radius")` failed with raw `-5`, which
C++-side reads as `Full` (pool exhausted) — the real code was
`NROS_RET_ALREADY_EXISTS` (launch-seeded param re-declared; benign, now
adopted as the rclcpp override case in `component_node.hpp`).

## Fix directions

Either (a) make the C++ `ErrorCode` numbering identical to `nros_ret_t`
(breaking for any C++ user matching on raw values), or (b) add an explicit
`Result::from_c(nros_ret_t)` translation and forbid the implicit raw
construction from C codes. A `static_assert` table pinning the shared codes
would prevent re-divergence either way.
