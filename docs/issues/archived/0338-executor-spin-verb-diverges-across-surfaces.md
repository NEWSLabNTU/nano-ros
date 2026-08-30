---
id: 338
title: "`spin` means the opposite thing in C++ vs C/Rust/rclcpp, and the C executor registration verbs are half-renamed away from rclc"
status: resolved
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

## Part 1 LANDED (2026-07-28) — the C++ `spin` verb

`Executor` now matches rclcpp, the C API and Rust:

| verb | meaning |
| --- | --- |
| `spin(poll_ms = 10)` | blocks until this executor is shut down |
| `spin_for(duration_ms, poll_ms = 10)` | the bounded form |
| `spin(duration_ms, poll_ms)` | `[[deprecated]]` alias for `spin_for`, one release |

The two-argument deprecated overload is unambiguous against the new one-argument
`spin()`, so it only ever matches a call that meant the bounded verb — existing
code compiles and gets a diagnostic naming the replacement.

Exit condition for the new `spin()` is `shutdown()` on THIS executor — the
executor-scoped analogue of rclcpp exiting when its context is shut down.
`Executor` is the explicit (non-global) surface and deliberately does not depend
on the global `nros::ok()`, which is what the free-function `nros::spin()` uses.

`std_compat`'s `executor_spin(...)` chrono wrapper became `executor_spin_for`,
with the old name deprecated the same way. Only ONE in-tree caller existed.

**Guard:** `tests/compile/spin_verbs.cpp`, run by `just check cpp`. It is a
COMPILE-time probe because the defect was the SHAPE of the API — which arities
exist and what they mean — so the assertion that catches a regression is that
these calls type-check with these signatures. Mutation-checked: renaming the new
`spin()` away fails it with `no matching function for call to
'nros::Executor::spin()'`.

## Part 2 LANDED (2026-07-29) — the C `add_*` spelling

Ten entity-registration verbs renamed to rclc's spelling, joining the
already-correct `nros_executor_add_client`:

`add_{subscription, subscription_raw_with_info, subscription_in_group, timer,
timer_in_group, service, guard_condition, action_server, action_client,
time_triggered_dispatcher}`.

The old names survive one release as MACRO aliases in the hand-written
`<nros/executor.h>` shim (guarded by
`NROS_NO_DEPRECATED_EXECUTOR_REGISTER_ALIASES`), not as extra exported symbols:
no new ABI surface, and a recompile is all an in-tree or downstream consumer
needs. Code compiled against the old SYMBOLS must be rebuilt — stated in the
header.

39 in-tree C sources migrated to the new spelling.

**Deliberately NOT renamed:** `nros_executor_register_parameter_services` and
`_register_lifecycle_services`. Those register a service SET — a capability, not
an entity — and have no rclc counterpart, so C6's "additions are fine" applies
and `register` is the honest verb. Renaming them for symmetry would have made
the API less accurate, not more.

**Guard:** `nros-c/tests/compile/executor_verb_aliases.c`, run by `just check c`.
It takes function POINTERS to both spellings and asserts the alias resolves to
the SAME function, so a macro pointing at a nonexistent symbol fails at the gate
rather than at some consumer's link step. Mutation-checked: deleting one alias
fails with `'nros_executor_register_timer' undeclared`.

## Relationship to #329

#329 covers the bounded-spin *implementation* being duplicated four times across the
C++ header layer. This issue is about the *names and semantics* of the public verb.
They should be fixed together: once the loop moves behind one CFFI entry point
(#329), the naming decision here is the only thing left to settle.
