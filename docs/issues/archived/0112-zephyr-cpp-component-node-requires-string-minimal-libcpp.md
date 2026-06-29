---
id: 112
title: "`nros-cpp` `component_node.hpp` includes `<string>` unconditionally — fails on Zephyr minimal C++ lib (`<string>` absent)"
status: resolved
type: bug
area: core
related: [phase-242]
---

## Resolution

`<string>` was included under `#if defined(NROS_CPP_STD) || (__STDC_HOSTED__ + 0)` —
but `__STDC_HOSTED__` reports compiler *hostedness*, which is true even for a host
`g++` invoked with `-nostdinc++` against Zephyr's minimal libcpp (no `<string>`). Yet
`<string>` is consumed *only* by the `std::string`-keyed parameter overloads, which are
already gated on `#ifdef NROS_CPP_STD` — so the include was on a strictly broader gate
than its sole consumer.

Fix (`component_node.hpp`): moved the `<string>` include into its own `#ifdef
NROS_CPP_STD` block, so it follows its actual consumer. `<cstdio>` (the hosted `fprintf`
boot diagnostic, which Zephyr minimal libcpp *does* provide) stays on the hosted gate.
Logically airtight — when `NROS_CPP_STD` is undefined the `std::string` overloads don't
exist, so nothing needs `<string>`.

**Verified end-to-end on the affected dev box:** `build-test-fixtures` now builds all six
Zephyr C++ XRCE entries (talker, listener, service-server, service-client, action-server,
action-client) to `zephyr.exe` + `zephyr.elf`; `zephyr_entry_main.cpp.obj` (which includes
`component_node.hpp`) compiles cleanly against the minimal libcpp — the
`string: No such file or directory` failure is gone. Zephyr Rust + C fixtures unaffected
(they were already green after #111).

## Summary

`packages/core/nros-cpp/include/nros/component_node.hpp:98` includes `<string>`
unconditionally:

```cpp
#include <string> // std::string-keyed parameter overloads (242.7 — rclcpp keys on std::string)
```

The **Zephyr C++ fixtures** (`examples/zephyr/cpp/*`) compile against Zephyr's
**minimal C++ library** — the compile line carries `-nostdinc++ -isystem
.../zephyr/lib/cpp/minimal/include -std=c++14 -fno-exceptions -fno-rtti`. That minimal
libcpp provides only a tiny subset of the standard headers and has **no `<string>`**, so
every C++ Zephyr entry fails:

```
component_node.hpp:98:10: fatal error: string: No such file or directory
```

Discovered building `build-test-fixtures` after #111 unblocked the Rust/C zephyr fixtures:
all 12 Rust and 12 C zephyr fixtures now build; the 12 **C++** ones (`build-cpp-*`) fail
here at `zephyr_entry_main.cpp` → `component_node.hpp`.

## Impact

Zephyr C++ entries cannot compile at all when built against the minimal libcpp. This blocks
the zephyr leg of `build-test-fixtures` (and therefore `just ci`'s `test-all` /
`cyclonedds-ci`) on any setup that doesn't supply a fuller C++ standard library to the
Zephyr build.

## Root cause

The phase-242.7 `std::string`-keyed parameter overloads pulled `<string>` into a header that
is included by every C++ entry, including embedded ones that link Zephyr's minimal libcpp.
`std::string` (heap-backed, exceptions) is a poor fit for `-fno-exceptions` minimal-libcpp
embedded targets regardless.

## Fix ideas

- **Gate the `<string>` include + the `std::string`-keyed overloads** behind a capability
  feature (e.g. `nros-cpp` `full-libcpp` / a `NROS_CPP_HAS_STD_STRING` config) that is off for
  Zephyr minimal-libcpp builds; embedded entries use the `const char*`-keyed parameter API.
- **Or** enable a fuller C++ standard library for the Zephyr C++ fixtures
  (`CONFIG_LIB_CPLUSPLUS` + a libstdc++/newlib-backed config that provides `<string>`), if the
  target budget allows — but `<string>` on `-fno-exceptions` minimal targets is fragile, so the
  capability-gate is preferred.

## Notes

Separate from #111 (which fixed the sizes-probe path bug that previously masked this). The
Rust and C zephyr fixtures are unaffected — this is C++-only.
