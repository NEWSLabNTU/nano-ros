---
id: 1682
title: "`rclcpp::get_logger(const std::string&)` stores a pointer into its
  argument, so `get_name()` on a logger built from a temporary string dangles"
status: resolved
type: bug
area: [api, cpp]
severity: low
found: 2026-10-05
related: [1637, 1019]
resolved_in: "branch fix/1682-logger-name-dangles"
---

## Symptom

`rclcpp::Logger` (`packages/api/nros-cpp/include/nros/log.hpp`) holds its name
as a borrowed `const char* name_`. The `std::string` overload of
`rclcpp::get_logger` forwards `name.c_str()` to the `const char*` overload,
which stores that pointer:

```cpp
inline Logger get_logger(const std::string& name) { return get_logger(name.c_str()); }
```

So a ported line that builds the name on the fly reads freed memory:

```cpp
auto log = rclcpp::get_logger(node_name + ".planner");  // temporary std::string
log.get_name();                                           // dangling
```

Upstream's `rclcpp::Logger` owns a `std::string`, so the same line is fine
there. Logging through the macros is unaffected: records dispatch on the
interned `nros_log` handle, not on `name_`. Only `get_name()`, and anything
reading the name of a copy, is undefined behaviour.

## What is stated today

Issue 1637 wrote the bound onto the class's doc comment ("The name is a
BORROWED `const char*` ... That includes the `std::string` overload of
`rclcpp::get_logger`, which borrows `c_str()`"). That makes the hazard
documented, not fixed.

## Shape of a fix

The interned logger already owns a stable copy of the name (`nros_log`'s
dynamic-logger name arena), but the C surface offers only a copying
accessor (`nros_logger_get_name(logger, buf, len)`). A borrowing accessor
returning the interned `const char*` would let both `get_logger` overloads
store a pointer whose lifetime is the image's, and the borrowed-name bound
would disappear. Freestanding builds have no `std::string` overload, so the
`const char*` overload with a string literal, the common case, was never at
risk.

## Resolution (2026-10-05)

`rclcpp::Logger` now OWNS its name: `nros::FixedString<NROS_CPP_LOGGER_NAME_CAPACITY + 1>`,
default 128 bytes (`nros_node::names::MAX_RESOLVED_NAME_LEN`, the longest name
the stack resolves), overridable with `-DNROS_CPP_LOGGER_NAME_CAPACITY=<n>`.
Both constructors copy, so every `get_logger` overload and every copy of a
Logger is safe however short-lived its source.

**Not the fix this issue sketched**, for three reasons measured in the code:
the interned name in `nros_log` is a Rust `&'static str`, length-delimited
and NOT NUL-terminated (`pool::intern_name` copies exactly `len` bytes); a
logger `register_logger`ed from Rust carries a plain literal, with no
terminator either; and a name over `MAX_LOGGER_NAME_LEN` (48) never reaches
the arena — it resolves to the catch-all, whose name is not the one the
caller passed. A borrowing accessor would have had to answer all three. An
owned copy answers the question upstream's API asks — "the name I gave it" —
and needs no C-ABI change. `FixedString` rather than `std::string` keeps it
heap-free and present on freestanding targets; the residual bound is
truncation past the capacity, stated on the class.

Regression: `tests/compile/logger_names_and_levels_runtime.cpp` builds a
Logger from a buffer, overwrites the buffer, and checks `get_name()` — for
`get_logger(name)`, the one-argument constructor, and a copy (`just check cpp`).

Found on the way: #1675 made `nros::init_with_launch*` (node.hpp) call
`nros::global_handle()`, which was DEFINED only in `nros.hpp`, so `-Wall`
reported "inline function used but never defined" in any TU that included
`node.hpp` alone — a link error for the first such TU to call
`init_with_launch_auto`. The definition moved to `node.hpp`.
