---
id: 1682
title: "`rclcpp::get_logger(const std::string&)` stores a pointer into its
  argument, so `get_name()` on a logger built from a temporary string dangles"
status: open
type: bug
area: [api, cpp]
severity: low
found: 2026-10-05
related: [1637, 1019]
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
