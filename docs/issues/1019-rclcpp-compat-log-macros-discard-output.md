---
id: 1019
title: "Every `RCLCPP_*` log call in a ported C++ node is discarded on embedded, and
  `RCLCPP_*_STREAM` drops its message on every target"
status: open
type: bug
area: api, docs
related: [phase-379, issue-0589, rfc-0018]
---

## Problem

`rclcpp_compat.hpp` exists so a ported rclcpp node compiles. Its log macros
compile and then throw the output away.

Three defects, in order of severity.

### 1. On embedded, every `RCLCPP_*` log line vanishes

```
RCLCPP_INFO(logger, ...)   ->  (void)(logger); NROS_INFO(__VA_ARGS__)   rclcpp_compat.hpp:554
NROS_INFO(...)             ->  NROS_LOG_SINK("INFO", __FILE__, __LINE__, ...)   log.hpp:43
```

and `NROS_LOG_SINK` is defined twice, on `__STDC_HOSTED__ || NROS_CPP_STD`
(`log.hpp:29`):

```c
/* hosted */       fprintf(stderr, ...)
/* freestanding */ ((void)(level), (void)(file), (void)(line))     log.hpp:38
```

The freestanding arm is a **no-op**. So on Zephyr, FreeRTOS, NuttX and ThreadX
— the targets nano-ros exists for — a ported node's entire log output is
compiled away with no diagnostic. It works on the host, which is where anyone
would test the port.

### 2. `RCLCPP_*_STREAM` discards its message on every target

```c
#define RCLCPP_INFO_STREAM(logger, args) RCLCPP_INFO(logger, "%s", "")   :580
```

`args` is not referenced. `RCLCPP_INFO_STREAM(get_logger(), "x=" << x)`
compiles and logs an **empty string** — a blank line on the host, nothing on
embedded. The stream form is idiomatic rclcpp, so this is not a rare path.

### 3. The whole family bypasses `nros_log`

Even on the host the output is a raw `fprintf(stderr)`, so severity thresholds,
per-logger levels, and the sink chain do nothing for exactly the code a port
produces. `logger` is cast to `void` at `:555`, so `rclcpp::get_logger("x")`
cannot select anything either — `rclcpp_compat.hpp:208-227` says it discards
the name.

`_THROTTLE` is the documented-but-still-wrong case: it degrades to the plain
macro (`:584-586`, comment at `:548`), dropping `clock` and `period_ms`
un-evaluated, so a 1 Hz throttle in ported code floods at loop rate.

## Why it survived

The comment at `log.hpp:12` states the hosted/freestanding split as a feature,
and it is a reasonable one for `NROS_INFO` — a board's own console macro should
compile out where there is no console. What is wrong is routing `RCLCPP_*`
through it: a ported node calling `RCLCPP_INFO` is not asking for a
board-console print, it is asking for the ROS logger, and `nros_log` reaches
`LOG_ERR`/`printk` on exactly the targets where this sink is a no-op.

That is the same asymmetry as issue 0589 one layer up: `nros_log` is the thing
that works on embedded, and the paths that look most natural do not use it.

## Fix

Re-point the `RCLCPP_*` family at the `NROS_LOG_*` dispatcher (which reaches
`nros_log` through the C API) instead of the legacy `NROS_LOG_SINK` family, so
a ported node's logs obey levels and reach sinks on every target. Then:

* implement `RCLCPP_*_STREAM` for real, or `#error` on it — silently dropping a
  message is worse than not compiling;
* implement `_THROTTLE` once C has a throttle (it does not today);
* make `rclcpp_compat::get_logger(name)` resolve a named logger rather than
  discard the name, which needs `nros_log_get_logger` in the C API first.

The C-side prerequisites (`nros_log_get_logger`, per-logger level, throttle,
`nros_log_add_sink`) are tracked as part of the phase-379 logging convergence;
this issue is the C++ half and is the one with silent data loss.
