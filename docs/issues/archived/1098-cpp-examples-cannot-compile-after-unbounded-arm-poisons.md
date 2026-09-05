---
id: 1098
title: "`std_msgs/String` on a C++ receive path stopped compiling: the 0964 poison arm landed with no in-tree consumer migrated"
status: resolved
type: bug
area: build, api
severity: high
found: 2026-09-05
related: [0964, 1075, phase-403]
---

# The poison arm is right; the tree was not measured for it

## Symptom

`just build-test-fixtures lane=tier2` fails the whole `native` family on the
first C++ example it reaches:

```
examples/native/cpp/listener/build-zenoh/nano_ros_cpp/std_msgs/msg/std_msgs_msg_string.hpp:82:93:
error: static assertion failed: NROS_UNBOUNDED__std_msgs_msg_string__field_data:
std_msgs/String states no serialized-size bound -- unbounded member: data (string).
```

required from `nros::Subscription<std_msgs::msg::String>::take`, at
`examples/native/cpp/listener/src/main.cpp:80`.

Every other family in the lane passes:

```
== zephyr == OK          == nuttx == OK
== threadx_linux == OK   == threadx_riscv64 == OK
== freertos == OK        == esp32 == OK
== qemu == OK            == native == FAILED (rc=2)
```

## Cause

`7d481d461` (*"fix(#0964): an unbounded type cannot size a buffer — the arm
poisons"*, main's HEAD) flipped `buffer_bounds<M, bound_shape::unbounded>` from
the estimate to `strict_bounds`:

```c++
// before
static constexpr size_t tx = M::SERIALIZED_SIZE_MAX;
static constexpr size_t rx = M::SERIALIZED_SIZE_MAX;
// after
static constexpr size_t tx = strict_bounds<M>::tx;
static constexpr size_t rx = strict_bounds<M>::rx;
```

`std_msgs/String` is `string data` — unbounded, and carries no `cap`. There is
**no `nros-codegen.toml` anywhere under `examples/`**, so no example supplies
one, and no example calls the `_sized` form. The two documented escapes are both
unused in the tree.

**The decision itself is not in question.** A buffer sized from an invented
number is a build fact and belongs at build time; the arm should poison. What
did not happen is the consumer migration that the flip requires.

## Why the commit measured zero

Its own COST IN-TREE paragraph reads:

> compile-check-fixtures builds 36 rows across 5 builders with zero `states no
> serialized-size bound` errors; check-cpp, check-c and check fast (213/213) pass.
> Every in-tree C++ consumer either uses a bounded type on these paths or does not
> reach them.

Those 36 rows do not include the example leaves. `examples/native/cpp/listener`
is a `fixtures.toml` row, built by `build-test-fixtures`, which no gate in the
push or merge lane runs — the same shape as issue 0319 and issue 1025, where a
lane nobody runs per-PR held the only evidence.

The commit message anticipates this exact failure mode one paragraph earlier,
about a different lane:

> A zero from a check that did not execute is not a zero.

## Blast radius is 13 leaves, not 1, and 12 are UNMEASURED

`std_msgs::msg::String` appears in 13 C++ example leaves:

```
examples/native/cpp/{listener,talker}
examples/qemu-arm-freertos/cpp/{listener,talker}
examples/qemu-arm-nuttx/cpp/{listener,talker}
examples/qemu-riscv64-threadx/cpp/{listener,talker}
examples/threadx-linux/cpp/{listener,talker}
examples/zephyr/cpp/{listener,talker}
examples/templates/cpp-port-minimal-publisher
```

**The green embedded verdicts above do not clear the other 12.** Those leaves
were not built in `lane=tier2` — `fx8.log` records `skip generate-rust:
examples/zephyr/cpp/listener`, and the generated header in
`zephyr-workspace/build-cortex-m-cpp-talker-zenoh/nano_ros_cpp/std_msgs/msg/
std_msgs_msg_string.hpp` is dated **2026-08-13**, three weeks before the poison
existed. A stale artifact that was never recompiled reports OK for the same
reason the 36 rows did.

Expect `lane=all` / the nightly to fail all 13. `std_msgs::msg::Int*` (86 uses)
is fixed-size and unaffected; `sensor_msgs::msg::Imu`, `geometry_msgs::msg::Point`
and the `Marker` uses are unchecked.

## Fix — two options, and the choice is a product decision

The commit deliberately left this open ("the migration a user would have to make
is a product decision, not one a header can take on their behalf"), so this issue
records the options rather than picking one:

1. **Cap the field per leaf.** A `nros-codegen.toml` beside each example's
   `package.xml`:

   ```toml
   [fields]
   "std_msgs/String.data" = 256
   ```

   **VERIFIED** on `examples/native/cpp/listener`: with that file the leaf
   compiles and links again (`[9/14] Linking CXX executable cpp_listener`).

   It is **13 files, not one shared file** — `_nros_generate_declared_interfaces`
   (`cmake/NanoRosVerbs.cmake:79`) reads only
   `${CMAKE_CURRENT_SOURCE_DIR}/nros-codegen.toml` and there is no walk-up, by
   design: "beside package.xml is the discoverable place". So the number is
   stated 13 times, and the 13 copies can drift.
2. **`bind_subscription_sized` / the `_sized` form per call site.** Keeps the
   leaves free of a config file, but the byte count is repeated at ~39 sites and
   each example grows an argument whose purpose needs explaining in the example
   that is supposed to be the simplest one.

**DECIDED 2026-09-05: option 1.** All 13 leaves get the cap at 256. After issue
0964 an example arguably SHOULD teach a bound, and a config file beside
`package.xml` says it once per leaf where a `_sized` argument would say it at
every call site. 256 is generous rather than tight — the payloads are one short
line — because the number is a demo's illustration, not a measurement.

The 13-copy drift risk is real and NOT fixed here. If it bites, the answer is
walk-up discovery in `_nros_generate_declared_interfaces`, not a 14th copy.

## Not covered

* The non-`String` unbounded types (`Imu`, `Point`, `Marker`) — not reached,
  because the build stops at the first failure.
* Whether the C (not C++) examples have the same exposure.
* A gate. The structural gap is that `build-test-fixtures` runs in no
  merge-gating lane, which is bigger than this issue.
