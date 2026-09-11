---
id: 1302
title: "`NROS_RCLCPP_REFUSE_THROTTLE` and its five ledger rows say there is no C
  throttle; phase-417 W4.d shipped one"
status: open
type: tech-debt
area: [api, docs]
related: [1019, phase-417, rfc-0089, rfc-0019]
---

## What

`NROS_RCLCPP_REFUSE_THROTTLE`
(`packages/api/nros-cpp/include/nros/log.hpp`) is the message a ported
`RCLCPP_INFO_THROTTLE(...)` gets. It says:

> There is no throttle on the C or C++ logging path; nros-log has one Rust-side
> and re-exporting it is phase-417 W4.d, so a throttle written here would be a
> second implementation of behaviour Rust already owns (RFC-0019).

**Phase-417 W4.d landed.** `packages/api/nros-c/include/nros/log.h` defines
`NROS_LOG_THROTTLE_AT` plus `NROS_LOG_{TRACE,DEBUG,INFO,WARN,ERROR,FATAL}_THROTTLE`,
backed by the exported `nros_log_throttle_admit` — which is `nros_log::throttle_decide`,
the same rule the Rust `nros_*_throttle!` macros use. So the sentence a porting
user reads is false about the tree they are building against, and the clause it
rests on ("re-exporting it is phase-417 W4.d") names work that is done.

The same text is quoted, near-verbatim, in five ledger rows:
`cpp:RCLCPP_DEBUG_THROTTLE`, `cpp:RCLCPP_INFO_THROTTLE`,
`cpp:RCLCPP_WARN_THROTTLE`, `cpp:RCLCPP_ERROR_THROTTLE` and
`cpp:RCLCPP_FATAL_THROTTLE` in `docs/reference/api-parity-ledger/other.json`.

## Why the refusal is still right, and this is only the reason

Do not read this as "so implement the macro". The refusal stands on a DIFFERENT
argument, which the message does not currently make:

`RCLCPP_INFO_THROTTLE(logger, clock, period, ...)` measures its window on the
CLOCK the caller passes. `nros_log_throttle_admit` measures on `nros_log`'s own
clock (`nros_log_timestamp_available()`; 0 everywhere without the
`platform-clock` feature, where each site emits once and then never — issue
1152). Forwarding the macro and dropping `clock` would compile and differ, which
is the original defect of this family (issue 1019) one argument over. A
`ros_time` throttle driven by `/clock` is a different capability, and it is not
one the C surface has.

So: rewrite the message to name the real constraint (the clock argument, not the
absence of a throttle), name `NROS_LOG_*_THROTTLE` as the un-clocked alternative
a caller can reach directly, and re-verdict the five rows to match. The
`ros_time`-clocked form, if it is ever wanted, is Rust-side work first
(RFC-0019).

## Why it survived

`api-parity.py --check` cannot reach these rows at all — neither extractor emits
C/C++ macros, which every one of the five rows already says about itself. Nothing
compares a refusal's PROSE against the tree, and nothing can: the claim is about
what exists elsewhere in the repo, not about the declaration the refusal sits on.
Found by reading, during phase-417 stage 3's fix of issue 1019, which is what
made `NROS_LOG_*` the family's route in the first place.

## Acceptance

* `NROS_RCLCPP_REFUSE_THROTTLE` names the clock argument as the constraint and
  `NROS_LOG_*_THROTTLE` as the alternative, and makes no claim about a throttle
  not existing.
* The five `cpp:RCLCPP_*_THROTTLE` rows agree with it.
* `ros2_refuse_log_throttle_probe.cpp` still fails with `REFUSED by nano-ros`
  (the refusal is unchanged; only its reason is).
