---
id: 1245
title: "The free `nros::spin(duration_ms, poll_ms)` is issue 0338's bounded-spin
  trap, one level up — 0338 renamed the Executor method and left the free
  overload behind"
status: open
type: bug
area: api, cpp
related: [rfc-0089, phase-427]
---

## Problem

Issue 0338 moved `Executor::spin()` to rclcpp's meaning (block until this
executor is shut down) and renamed the old bounded two-argument form to
`spin_for`, because — its own words — *"a user porting rclcpp code wrote
`exec.spin()` and it did not compile; reaching for `spin(ms)` instead silently
returned early"*. It also `[[deprecated]]`'d the free `executor_spin(exec, ms,
ms)` onto `executor_spin_for`.

It did not sweep the FREE global-executor form. Today
`packages/api/nros-cpp/include/nros/nros.hpp` ships both:

```cpp
inline Result spin();                                       // :170 — rclcpp's meaning
inline Result spin(uint32_t duration_ms, int32_t poll_ms = 10);  // :190 — bounded
```

plus the `std::chrono` overload of the bounded one at `std_compat.hpp:238`.

So the no-argument form is upstream's contract and correct, and the exact reach
0338 named — a user who wants "spin for a while" typing `nros::spin(1000)` —
still lands on a bounded spin under a name whose sibling means "forever". The
C++ overload set keeps this from being a *silent* contract inversion (a ported
`rclcpp::spin(node)` gets no matching overload, which is mechanical per
RFC-0089), so this is a naming inconsistency across our own surfaces rather than
a compile-or-conform violation. It is still the same trap 0338 decided was worth
a rename, and the class fix was left half-applied — the recurrence pattern
CLAUDE.md calls out.

## Evidence

Three live call sites of the bounded free form, all internal:

* `packages/api/nros-cpp/include/nros/main.hpp:116` — `::nros::spin(bound_ms)`
* `packages/api/nros-cpp/include/nros/nros.hpp:1170` — the `rclcpp::Rate`
  forwarder, `::nros::spin(remaining_ms, poll_ms)`
* `packages/api/nros-cpp/tests/compile/ros2_one_dispatch_path.cpp:163` —
  `(void)::nros::spin(10, 5);`

The ledger records the executor-side half (`cpp:executor_spin` as a
`[[deprecated]]` alias for `cpp:executor_spin_for`) and has no row for the free
one, because the correlator scores the NAME `spin` as `same` against
`rclcpp::spin` and never looks at the overload set. That is why the sweep could
be half-done and stay green.

## Fix

The shape 0338 already chose, applied to the free form:

1. add `inline Result spin_for(uint32_t duration_ms, int32_t poll_ms = 10)` to
   `nros.hpp` (forwarding to `nros_cpp_spin_for`, the same CFFI entry the
   bounded `spin` already uses) and `spin_for(std::chrono::milliseconds,
   std::chrono::milliseconds)` to `std_compat.hpp`;
2. move the three call sites;
3. `[[deprecated("bounded spin is now `nros::spin_for(...)` (issue 0338)")]]`
   on the bounded `spin` overloads for one release, exactly as `executor_spin`
   carries today;
4. a `cpp:spin_for` ledger row, and a note on `cpp:spin` that the correlator's
   `same` verdict is about the name and not the overload set.

## Why it is not fixed here

Found during phase-427's Rust spin-family rename. `nros.hpp` is the header
PR #755 (phase-427's C++ node merge) is rewriting, and a deprecation attribute
landing in it mid-merge is a conflict for no urgency — nothing is broken today,
only misnamed. It belongs to the C++ wave that owns that header.
