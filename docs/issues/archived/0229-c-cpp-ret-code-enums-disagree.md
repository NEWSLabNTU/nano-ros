---
id: 229
title: "C nros_ret_t and C++ nros::ErrorCode disagree from -5 down — Result(C code) misreports ALREADY_EXISTS as Full"
status: resolved
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

## Resolution (2026-07-18) — one numbering, pinned

Chose alignment (fix direction a): the C++-space now uses the SAME numbering
as the canonical C `nros_ret_t`, so `Result(<any C-ABI return code>)` is
correct by identity. nros-c's `NROS_RET_*` was already canonical and is
UNCHANGED; the drift was entirely in the C++ side.

- `nros::ErrorCode` (result.hpp) renumbered to mirror `nros_ret_t` exactly:
  NotFound=-4, AlreadyExists=-5 (new), Full=-6, NotInitialized=-7,
  BadSequence=-8, ServiceFailed=-9, PublishFailed=-10,
  SubscriptionFailed=-11, NotAllowed=-12, Rejected=-13, TryAgain=-14,
  Reentrant=-15, Unsupported=-16, TransportError=-100. Pre-#229: Full=-5,
  TryAgain=-6, Reentrant=-7 — so a raw `-5` (`ALREADY_EXISTS`) read as
  `Full`.
- `nros_cpp_ret_t` FFI constants (nros-cpp/src/lib.rs) renumbered to match;
  `nros_cpp_ffi.h` regenerated.
- Re-divergence tripwires: `static_assert` pin tables in result.hpp
  (self-consistency), parameter.hpp (vs the C `NROS_RET_*` macros), and
  node.hpp (vs `NROS_CPP_RET_*`). Proven live: a deliberate `ErrorCode`
  break fails BOTH the self-consistency and cross-space asserts with the
  #229 message; the guards compile clean at the true values and the
  parameter.hpp pin is active in the real (nros-c include-path) compile.

Verified: nros-cpp builds + clippy clean; `check-c` cross-include TU
(compiles the pins) green; param + service nextest lanes 26/26 on rebuilt
fixtures. No `switch(ErrorCode)` and no raw-value matchers exist to break.
Behavior correctness follows by identity — `Result(NROS_RET_ALREADY_EXISTS)`
now yields `ErrorCode::AlreadyExists`; the static_asserts are the durable
guard against re-divergence.
