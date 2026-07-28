---
id: 338
title: "`spin` means the opposite thing in C++ vs C/Rust/rclcpp, and the C executor registration verbs are half-renamed away from rclc"
status: open
type: bug
severity: medium
area: core, api
related: [issue-0329, rfc-0018, rfc-0019]
---

## Finding (deep audit C,E 2026-07-28 — C6, upstream-anchored)

### 1. One name, three meanings

| surface | signature | meaning |
| --- | --- | --- |
| C++ `Executor::spin` (`nros-cpp/include/nros/executor.hpp:139`) | `spin(duration_ms, poll_ms)` | **bounded**; no no-arg overload exists |
| C `nros_executor_spin(executor)` | — | runs **forever** |
| Rust `Executor::spin(timeout) -> !` | + `spin_default()` | runs **forever**, per-iteration timeout |
| C++ free function `nros::spin()` (`nros.hpp:78`) | — | blocks until `!ok()` |

**Upstream counterpart:** `rclcpp::Executor::spin()` blocks until shutdown; the
bounded verb is `rclcpp::Executor::spin_some(max_duration)`. So the one surface that
is supposed to mirror rclcpp is the one where `spin` means "bounded", and it is the
only surface with no way to say "spin forever" on an executor.

A user porting rclcpp code writes `exec.spin()` — which does not compile in C++
here, and in Rust means something subtly different from the C++ `spin(ms)` they
might reach for instead.

### 2. The C registration family is half-renamed away from rclc

`nros-c/include/nros/nros_generated.h:4168` — the C executor renames rclc's
`rclc_executor_add_*` family to `nros_executor_register_*` for **seven** entity
kinds, but leaves **`nros_executor_add_client`** on the rclc spelling. Every doc
comment in the family still reads "Add a X to the executor".

**Upstream counterpart:** `rclc_executor_add_subscription` /
`_add_timer` / `_add_client` / `_add_service` / `_add_guard_condition`.

Per C6, additions are fine but renames of standard concepts are drift — and this
rename is not even internally consistent, so a C user cannot guess the verb.

## Fix

1. Add `Result spin()` to the C++ `Executor` with rclcpp semantics (loop `spin_once`
   until `!ok()`), rename the bounded overload to `spin_for(duration_ms, poll_ms)`
   (or `spin_some(max_duration)` to match rclcpp exactly), and keep the current
   signature as a deprecated alias for one release.
2. Settle the C family on the rclc spelling (`nros_executor_add_*`), keeping
   `register_*` as deprecated `static inline` aliases so existing C code still
   compiles, and mirror the chosen verb in the C++/Rust registration seams' doc text.

## Relationship to #329

#329 covers the bounded-spin *implementation* being duplicated four times across the
C++ header layer. This issue is about the *names and semantics* of the public verb.
They should be fixed together: once the loop moves behind one CFFI entry point
(#329), the naming decision here is the only thing left to settle.
