---
id: 2881
title: "No compile-time log floor (`RCLCPP_LOG_MIN_SEVERITY`): a filtered-out `RCLCPP_DEBUG` still evaluates its arguments in the control loop"
status: open
type: enhancement
severity: low
area: [cpp, logging, api]
related: [phase-478, 2636]
found: 2026-11-23
---

**Home:** [phase-478](../roadmap/phase-478-island-developer-experience-findings.md)
I41. Consumer: the safety island (autoware-safety-island fork), branch
`nano-ros-next`, nano-ros pin `8d4e70a382`.

## What happens

`RCLCPP_DEBUG` and the other levels are filtered at run time, by the logger's
threshold (`packages/api/nros-cpp/include/rclcpp/logging.hpp`: "`RCLCPP_DEBUG`
no longer compiles out under `NDEBUG`. It is runtime-filtered by the logger's
threshold now"), and `knobs.log.max_level` is a run-time ceiling `nros_log`
checks; by then the call site has evaluated every argument. Upstream rclcpp
also has a compile-time floor: defining
`RCLCPP_LOG_MIN_SEVERITY` to one of `RCLCPP_LOG_MIN_SEVERITY_{DEBUG,INFO,WARN,
ERROR,FATAL,NONE}` makes every macro below it expand to nothing
(`rclcpp/logging.hpp`, generated from `logging.hpp.em`). nano-ros's header
has no such symbol.

On a 30 ms control loop that matters: the vendored Autoware controllers log at
debug level on every cycle (upstream spells it `RCLCPP_DEBUG`), with
arguments that compute.

## Island workaround

The island keeps its own logging shim for the level:

- `actuation_module/src/asi_common/include/common/logger/logger.hpp`: `log_*`
  functions gated by `#if CONFIG_LOG_LEVEL >= N` (asi_common's own Kconfig
  symbol), "zero-cost below level", and `PROFILE_*` compiled out below 2.
- Its phase-15 P4 records it as waiting on nano-ros: "the level stays
  `CONFIG_LOG_LEVEL` (compile time ...). `knobs.log.max_level` is a ceiling
  nros_log checks at run time, after the call site has evaluated its
  arguments, so it would not keep the per-cycle debug work out of the
  control loop."

## Wanted

`RCLCPP_LOG_MIN_SEVERITY` and the six `RCLCPP_LOG_MIN_SEVERITY_*` values with
upstream's meaning, every `RCLCPP_*` family (plain, `_ONCE`, `_THROTTLE`,
`_SKIPFIRST`, `_EXPRESSION`, `_FUNCTION`, `_STREAM`) compiled out below it,
arguments unevaluated. Optionally a knob (`log.min_compiled_level`) that sets
it per image, so a board image does not need a `-D` in a package's CMake.

## Acceptance

- `just check cpp` holds: with `RCLCPP_LOG_MIN_SEVERITY=RCLCPP_LOG_MIN_SEVERITY_INFO`,
  an `RCLCPP_DEBUG(logger, "%d", f())` does not call `f`; at `_DEBUG` it does.
- The book's rclcpp compatibility page lists the symbol.
- The island maps `log_*` onto `RCLCPP_*` with the floor set from its image,
  and deletes `CONFIG_LOG_LEVEL` gating from `logger.hpp`.
