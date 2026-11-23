---
id: 2881
title: "No compile-time log floor (`RCLCPP_LOG_MIN_SEVERITY`): a filtered-out `RCLCPP_DEBUG` still evaluates its arguments in the control loop"
status: resolved
type: enhancement
severity: low
area: [cpp, logging, api]
related: [phase-478, 2636]
found: 2026-11-23
resolved: 2026-11-24
resolved_in: "feat(log): RCLCPP_LOG_MIN_SEVERITY and RCLC_LOG_MIN_SEVERITY compile-time floors"
---

**Home:** [phase-478](../../roadmap/phase-478-island-developer-experience-findings.md)
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

## Resolution

- `packages/api/nros-cpp/include/rclcpp/logging.hpp` defines
  `RCLCPP_LOG_MIN_SEVERITY_{DEBUG,INFO,WARN,ERROR,FATAL,NONE}` (0..5, upstream's
  values) and defaults `RCLCPP_LOG_MIN_SEVERITY` to `_DEBUG`. Each level has
  one gate, `NROS_RCLCPP_AT_<LEVEL>_(m)`, which is `m` at or above the floor
  and an empty `do { } while (0)` below it; every family that exists here is
  written through it: plain, `_ONCE`, `_SKIPFIRST`, `_THROTTLE`,
  `_SKIPFIRST_THROTTLE` and `_STREAM`. Below the floor the logger, the clock,
  the format arguments and the stream operands are not evaluated.
- The C API gets the twin, `RCLC_LOG_MIN_SEVERITY` in `<rclc/log.h>`, values
  `RCLC_LOG_MIN_SEVERITY_{TRACE,DEBUG,INFO,WARN,ERROR,FATAL,NONE}` = 0..6,
  the numbering of `log.max_level` (`NROS_LOG_MAX_LEVEL`). It gates
  `RCLC_LOG_<LEVEL>` (to `((void)0)`) and `RCLC_LOG_<LEVEL>_THROTTLE`; as
  rcutils' floor does upstream, it also applies to `RCLCPP_*`, so a level
  survives only at or above both. An out-of-range value is an `#error`.
- How a project sets it: a compile definition, upstream's way, which a ported
  package's CMake already carries (`target_compile_definitions`, or
  `zephyr_compile_definitions`). No knob: the C/C++ headers have no per-image
  carrier on every road (the CMake road hands image knobs to cargo only), and a
  floor that worked on one road would differ by road. On Zephyr the image's
  ceiling can be passed through unchanged, since the numbering matches:
  `-DRCLC_LOG_MIN_SEVERITY=CONFIG_NROS_LOG_MAX_LEVEL`.
- Not added: `_EXPRESSION` and `_FUNCTION` (not provided here at all; see
  issue 2636).

## Test

`packages/api/nros-cpp/tests/compile/log_min_severity_runtime.cpp`, run by
`just check cpp` (`cpp-runtime`) in four floor arms (unset,
`RCLCPP_..._INFO`, `RCLCPP_..._NONE`, `RCLC_..._WARN`), each with and without
`NROS_CPP_STD`, under `-Wall -Wextra -Werror`. Every operand goes through a
counting function. For every level of every family it holds: below the floor
nothing is evaluated and nothing reaches the sink; at or above it the operands
are evaluated and one record arrives at its own severity. The issue's line,
`RCLCPP_DEBUG(logger, "%d", f())`, is checked on its own.
