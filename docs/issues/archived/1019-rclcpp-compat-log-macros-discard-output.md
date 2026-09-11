---
id: 1019
title: "Every `RCLCPP_*` log call in a ported C++ node is discarded on embedded, and
  `RCLCPP_*_STREAM` drops its message on every target"
status: resolved
type: bug
area: api, docs
related: [phase-379, phase-417, issue-0589, rfc-0018, rfc-0089]
---

## Resolution — the family routes at `NROS_LOG_*` (2026-09-11, phase-417 stage 3 W3.a)

All three defects had ONE cause, so they have one fix. `RCLCPP_INFO` / `_WARN` /
`_ERROR` / `_DEBUG` / `_FATAL` in `packages/api/nros-cpp/include/nros/log.hpp` no
longer expand to `(void)(logger); NROS_<LEVEL>(...)`. They expand to
`NROS_LOG_<LEVEL>(::rclcpp::detail::log_handle(logger), ...)`, which dispatches
through `nros_log`:

* **defect 1 (embedded silence)** — `nros_log` reaches `LOG_ERR`/`printk` on
  exactly the targets where the legacy `NROS_LOG_SINK` is a no-op. The legacy
  family is UNCHANGED and still correct for a board's own console print; what was
  wrong was routing `RCLCPP_*` through it, which is what this issue said.
* **defect 2 (`_STREAM` drops its message)** — closed earlier, by
  `NROS_RCLCPP_STREAM_` building the text through `std::ostringstream` and
  forwarding it as one `%s` argument. It now inherits the routing too, so the
  message reaches `nros_log` rather than `stderr`, and is present only where
  `<sstream>` is (a freestanding target gets `no member named`, which is honest).
* **defect 3 (bypasses `nros_log`; the logger is discarded)** —
  `rclcpp::get_logger(name)` RESOLVES the name through `nros_log_get_logger`
  instead of building a null-handled sentinel, and the macros carry the handle.
  Per-logger levels now apply to the family. The C-side prerequisite this issue
  named as blocking (`nros_log_get_logger`) landed in phase-417 W4.d.

Two things this fix had to avoid, both recorded because they are the near-misses:

* `nros_log_emit_fmt_at` RETURNS EARLY on a null handle, so re-pointing the
  family at it without more would have swapped one silent drop for another — a
  `Logger` built from a name alone, or from an uninitialised node, carries a null
  handle. `rclcpp::detail::log_handle` sends those to the catch-all `nros`
  logger.
* `RCLCPP_FATAL` lowered to `NROS_ERROR` because the legacy family has no fatal
  level at all. It emits at `NROS_LOG_SEVERITY_FATAL` now. That half is its own
  ledger row, `cpp:RCLCPP_FATAL` in `docs/reference/api-parity-ledger/other.json`,
  and it is closed in the same commit.

One consequence, named: `RCLCPP_DEBUG` no longer compiles out under `NDEBUG`. It
is runtime-filtered by the logger's threshold, which is what upstream does.

**`_THROTTLE` stays REFUSE-LOUD**, and this issue's fourth bullet is the reason to
say why rather than leave it implied. The refusal is still correct — upstream's
`clock` argument is load-bearing (the window is measured on THAT clock) and
`nros_log_throttle_admit` measures on `nros_log`'s own, so adopting the macro
would still silently ignore an argument, which is the same defect one level over.
But `NROS_RCLCPP_REFUSE_THROTTLE`'s stated REASON ("there is no throttle on the C
or C++ logging path") went stale when phase-417 W4.d landed
`NROS_LOG_*_THROTTLE` in `packages/api/nros-c/include/nros/log.h`. That message
belongs to the five `cpp:RCLCPP_*_THROTTLE` ledger rows and is left to their
owner; it is filed rather than edited here.

**Held by** `packages/api/nros-cpp/tests/compile/ros2_loudness_runtime.cpp`, on
the `just check cpp` lane. It links `libnros_cpp.a`, installs a real sink through
`nros_log_add_sink`, and asserts each record ARRIVES at the RIGHT SEVERITY
carrying its MESSAGE and its LOGGER NAME. A sink is the only observer that can
tell "the record reached `nros_log` at FATAL" from "something printed to stderr",
which is precisely the distinction this issue is about. Negative control, run
against the pre-fix headers: **12 failures and ZERO records captured**, with the
console showing `[ERROR] … fatal-marker 4`.

---

## Problem as filed (kept — the resolution above says what landed)

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
