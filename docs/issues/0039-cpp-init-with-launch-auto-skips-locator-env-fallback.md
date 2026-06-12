---
id: 39
title: C++ `init_with_launch_auto` skips the `NROS_LOCATOR`/`ROS_DOMAIN_ID` env fallback → null locator → TransportError
status: open
type: bug
area: c-api
related: [phase-239]
---

`nros::init_with_launch_auto()` (the Phase 212.L.5 launch-aware init) routes
through the **3-arg** `init(locator, domain_id, session_name)` overload with a
literal null locator:

```cpp
// packages/core/nros-cpp/include/nros/node.hpp
inline Result init_with_launch_auto(int argc, char** argv, const char* session_name) {
    const char* name = (session_name != nullptr) ? session_name : "nros_cpp";
    return init(nullptr, 0, name);   // <-- 3-arg overload, no env fallback
}
```

Only the **2-arg** `init(locator, domain_id)` performs the
`$NROS_LOCATOR` / `$ROS_DOMAIN_ID` `getenv` fallback (node.hpp:538). The 3-arg
overload calls `nros_cpp_init(locator, ...)` straight through, so a fresh build
that uses `init_with_launch_auto` (the documented launch-aware entry point)
passes a null locator to the backend and fails with `TransportError` (-100) at
init — unless `$NROS_RUNTIME_OVERLAY` or an explicit locator is supplied.

## Impact

- Any native example/app calling `init_with_launch_auto()` and relying on
  `$NROS_LOCATOR` (the harness default for native tests) fails to start.
- `examples/native/cpp/service-client/src/main.cpp` uses
  `init_with_launch_auto` and is **latently broken on rebuild**; its currently
  prebuilt fixture binary predates the regression (uses the explicit-locator
  2-arg `init`), masking it in CI.

## Discovery

Phase 239 Wave 4: the new `examples/native/cpp/service-client-callback` got
`init_with_launch_auto -> -100` deterministically. Worked around there by using
`nros::init()` (the env-fallback 2-arg form, as in talker/listener).

## Fix options

1. Make the 3-arg `init(locator, domain_id, session_name)` apply the same
   `NROS_LOCATOR` / `ROS_DOMAIN_ID` env fallback when `locator == nullptr` /
   `domain_id == 0` (preferred — single source of truth, both overloads agree).
2. Or have `init_with_launch_auto` resolve the env overlay itself before
   delegating.

Until fixed, native C++ examples should call `nros::init()` (no-arg) rather than
`init_with_launch_auto()` when they depend on the env locator.
