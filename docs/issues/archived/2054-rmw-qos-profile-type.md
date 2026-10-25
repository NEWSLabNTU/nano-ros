---
id: 2054
title: "No rmw_qos_profile_t type: code that names it, or reads its fields, does not compile"
status: resolved
type: enhancement
area: [api, cpp]
severity: low
found: 2026-10-25
resolved: 2026-10-25
related: [phase-488]
---

## What

Humble-era rclcpp code passes the rmw struct to `create_service`, as in
`create_service<S>(name, cb, rmw_qos_profile_services_default, group)`.
Phase-488 W3 added the five `rmw_qos_profile_*` constants on the std surface.
Each one is an `rclcpp::QoS` built from `rclcpp::detail::qos_table`, so it
binds to every verb's `QoS` parameter. Autoware's
`autoware_external_cmd_selector` @0.45.0 compiles unchanged with them.

There is still no `rmw_qos_profile_t` type. These forms fail to compile:

- `rmw_qos_profile_t profile = rmw_qos_profile_default;`
- `profile.depth = 1;`, or any other field read or write (`history`,
  `reliability`, `durability`, `deadline`, …);
- `rclcpp::QoS(rclcpp::QoSInitialization::from_rmw(profile), profile)`;
- `qos.get_rmw_qos_profile()`.

`rmw_qos_profile_system_default` is also absent, because every field of it is
a sentinel; `qos_table` leaves it out for the same reason.

## Done when

The std surface has an `rmw_qos_profile_t` whose field names and enum values
are upstream's. The five constants are values of that type and still convert
to `rclcpp::QoS`. `QoS` converts back through `get_rmw_qos_profile()`. A probe
in `just check cpp` compiles the four forms above. If the type is deliberately
not taken, the porting guide says so instead, together with the replacement
edit.

## Resolution

The type is taken. `packages/api/nros-cpp/include/rclcpp/qos.hpp` defines
these on every target, with upstream's field names, field
order, field types and enumerator values, checked against
`/opt/ros/humble/include/rmw/rmw/types.h`:

- `rmw_qos_profile_t` (tag `rmw_qos_profile_s`) and `rmw_time_t`;
- the four `rmw_qos_*_policy_t` enums;
- the `RMW_DURATION_*` and `RMW_QOS_*_DEFAULT` / `_BEST_AVAILABLE` macros.

What converts to what:

- **The five constants** are now values of that type. Each is still lowered
  from its `rclcpp::detail::qos_table` row, so the table remains the one place
  a profile's policies are spelled.
- **Profile to `QoS`:** a profile converts implicitly to `rclcpp::QoS`. That
  is how Humble's `create_service(..., const rmw_qos_profile_t&, ...)` call
  sites reach a verb that takes `QoS`.
- **`QoS` to profile:** `QoS::get_rmw_qos_profile()`. It returns by value,
  because `QoS` stores the C ABI's fields, not upstream's, so there is no
  struct to return a reference to.
- **`rclcpp::QoSInitialization`** has `from_rmw()`, and the
  `QoS(QoSInitialization[, profile])` constructors exist. A `QoS` converts to
  a `QoSInitialization`, so `rclcpp::QoS(rclcpp::KeepLast(n), profile)` also
  compiles, even though our `KeepLast` returns a whole `QoS`.
- **The two vocabularies number the policies differently.** In `rclcpp::`,
  `Reliable` is 0; upstream's `RELIABLE` is 1. So each policy is mapped by an
  explicit table, never by a cast.
- **Time windows round-trip.** `{0, 0}` means "no window" on both sides.
  `RMW_DURATION_INFINITE` and the best-available value map to the C ABI's two
  sentinels. Any other window is rounded up to the next millisecond, as
  `qos_window_ms` already does.

`packages/api/nros-cpp/tests/compile/rmw_qos_profile_type.cpp` compiles in
`just check cpp`. It covers the four forms listed above. It also states every
constant's fields, the conversion in both directions, and the window sentinels,
each as a `static_assert`. Breaking one value (the parameters depth) fails
the compile.

Every target, not only the std surface the constants started on: the
`cpp-capability-layout` gate requires a public `rclcpp::` type
(`QoSInitialization`) to exist without `NROS_CPP_STD` and on a freestanding
target (RFC-0096 D1), and nothing here needs the STL. So the block lost its
`NROS_CPP_STD` gate, and the `rmw-profile-names` hosted-family entry went
with it.

## Deliberate differences

- **`rmw_qos_profile_system_default` is still absent**, for the reason above:
  every field of it is a placeholder, and the reference RMWs fill it in
  differently (issue 0829).
- **The RMW ABI's `<nros/rmw_entity.h>` also defines a struct named
  `rmw_qos_profile_t`.** It is the backend vtable's, with ABI field widths.
  That header is for a backend's implementation, never for a node. A
  translation unit that includes it before `<rclcpp/qos.hpp>` gets an
  `#error` that names both headers. The reverse order fails with the
  compiler's redefinition error, which also names both files. No translation
  unit in the tree includes both.
