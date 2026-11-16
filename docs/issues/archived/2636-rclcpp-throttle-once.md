---
id: 2636
title: "RCLCPP_*_THROTTLE was refused and RCLCPP_*_ONCE / _SKIPFIRST were missing"
status: resolved
type: enhancement
area: [cpp, logging]
severity: medium
found: 2026-11-16
resolved: 2026-11-16
resolved_in: "feat(cpp): RCLCPP_*_THROTTLE on the clock argument, and the _ONCE / _SKIPFIRST families"
related: [phase-478]
---

**Home:** phase-478 I20 (found by the NXP island). GitHub #4010.

## Symptom

The island vendors about 25 Autoware controller files that log through
`RCLCPP_WARN_THROTTLE(get_logger(), *get_clock(), ms, ...)`. On nano-ros that
did not compile: every `RCLCPP_*_THROTTLE` expanded to
`rclcpp::detail::throttle_is_refused`, a `static_assert` carrying
`NROS_RCLCPP_REFUSE_THROTTLE`. There was no `RCLCPP_*_ONCE` or `_SKIPFIRST`
either. So the island wrote its own `log_*_throttle` over a `std::map` and a
pthread mutex, which a freestanding image cannot carry, or would have had to
edit every vendored call site to `RCLC_LOG_*_THROTTLE`.

## Why it was refused

Upstream measures the window on the `clock` argument. The C throttle,
`rclc_log_throttle_admit`, reads `nros_log`'s own clock, so forwarding the
macro to it would have dropped `clock` and compiled and differed. The refusal
was right about that; what was missing was a way to hand the throttle a
reading.

## Fix

- `rclc_log_throttle_admit_at(last_ns, interval_ms, now_ns)` (`<rclc/log.h>`):
  the same rule (`nros_log::throttle_decide`) and the same per-site
  `uint64_t`, on a reading the caller supplies.
- `RCLCPP_<LEVEL>_THROTTLE(logger, clock, period_ms, ...)` reads
  `clock.now()` and passes it there, so the window is measured on the clock
  the caller named, as upstream does. Every valid clock is honoured: steady,
  system, and ROS time both on system time and driven by `/clock`. The first
  record at a site logs, then at most one per `period_ms`.
- The one clock it cannot honour is an invalid one (`RCL_CLOCK_UNINITIALIZED`,
  whose `now()` reads zero). The site says
  `NROS_RCLCPP_REFUSE_THROTTLE_CLOCK_RUNTIME` at WARN, once, and is then
  throttled on the platform's monotonic clock (`rclc_log_throttle_admit`).
  Upstream lets every record through when its time read fails; this does not,
  because on an embedded target a throttled site is usually a hot loop, and
  letting it through floods the console and the transport (the maintainer's
  ruling). It is decided at runtime because a clock's type is a constructor
  argument, not part of its C++ type.
- `RCLCPP_<LEVEL>_ONCE`, `_SKIPFIRST` and `_SKIPFIRST_THROTTLE`, for DEBUG,
  INFO, WARN, ERROR and FATAL. Upstream's skip-first-throttle order is kept:
  the throttle admits and arms, then the first admitted record is dropped.
- Per-site state is function-local statics: no heap, no table, no lock. The
  `_ONCE` flag is an atomic load and store (`__atomic_*`, as `Node`'s error
  flag), not a read-modify-write, so no libatomic call. The logger's level is
  checked first, as rcutils does, so a filtered record spends no `_ONCE` and no
  window.

Two documented differences from upstream, both from the shared rule: the first
record at a site always logs (upstream's first comparison is
`now >= 0 + duration`, which drops it while a simulated clock reads below the
window), and a clock that jumps backwards admits a record where upstream would
wait for the clock to catch up.

## Test

`packages/api/nros-cpp/tests/compile/log_conditional_runtime.cpp`, run by
`just check cpp` with and without `NROS_CPP_STD`: THROTTLE on a ROS clock the
test drives through the override (logs, suppressed inside the window, logs at
the window), on a real steady clock, and on a ROS clock on system time; ONCE
once over ten calls; SKIPFIRST; SKIPFIRST_THROTTLE; a filtered call spending no
ONCE; an invalid clock still throttled (once inside the window, again after
it on the monotonic clock) and named once at WARN; and all twenty macros at their own severity.
The old negative probe `ros2_refuse_log_throttle_probe.cpp` is deleted.

Not added: the `_STREAM_ONCE` / `_STREAM_THROTTLE` crosses, `_EXPRESSION` and
`_FUNCTION`.
