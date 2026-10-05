---
id: 1678
title: "C++ `declare_parameter<long long>` does not compile on LP64 hosts: the declare/get overload set has `int64_t` (= `long`) but not `long long`"
status: resolved
type: bug
area: [api]
severity: low
found: 2026-10-05
related: [1649, phase-446]
resolved_in: "branch issue-1678 (fix(#1678) PR)"
---

## What

`Node::declare_parameter<T>` (`packages/api/nros-cpp/include/nros/nros.hpp`)
forwards to `nros::detail::node_param_declare(h, name, T)` and
`node_param_get(h, name, T&)` (`node_parameters.hpp`), an OVERLOAD set:
`bool`, `int64_t`, `int`, `double`, `const char*` (plus the std/Seq forms).

On an LP64 host `int64_t` is `long`, so `T = long long` matches none exactly:
the declare call is AMBIGUOUS between the `bool` / `int64_t` / `int` / `double`
conversions, and `node_param_get(..., long long&)` has no viable overload at
all. Hit 2026-10-04 writing a temporary parameter into
`examples/workspaces/cpp/src/talker/src/Talker.cpp` for issue 1649's
measurement; `declare_parameter<int64_t>` compiled.

The SAME header already knows `long long` is an integer: `node_param_type`
specialises `long long` and `unsigned long long` as `INTEGER` (and `long`,
`short`, `char`…), so the contract check accepts a type the store calls
cannot take. The type table and the overload set disagree.

On 32-bit ARM `int64_t` is `long long`, so the same source compiles there and
fails on the host — the portable direction is the wrong way round for a
header whose callers write rclcpp-style code once.

## Fix direction

Make the overload set cover what `node_param_type` claims, or narrow the
table to what the overloads take. One way: a template overload for integral
`T` that widens to `int64_t` (and its `get` twin narrowing with a range
check), keyed on the same list as `node_param_type` — no `<type_traits>`,
since the header is parsed under `-nostdinc++`. Test: a `check-cpp` TU that
declares `long long`, `long`, `int64_t` and `short` parameters on the host
and on an ILP32 target.

## Resolution (2026-10-05)

**The class was wider than the report, in both directions.** Measured against
the old header with a one-line TU per type (`c++` = LP64 host,
`arm-none-eabi-g++ -mcpu=cortex-m3` = ILP32):

| `declare_parameter<…>` | LP64 before | ILP32 before | after (both) |
| --- | --- | --- | --- |
| `long long` | FAIL | OK | OK |
| `long` | OK | **FAIL** | OK |
| `float` | FAIL | FAIL | OK |
| `Seq<int, 2>` | FAIL | FAIL | OK |

`long` was the mirror-image of the report — the portable direction ran BOTH
ways, so a node written on a Cortex-M broke on the host and vice versa. `float`
(DOUBLE in the table) and every array whose element is not the store's slot
type (`Seq<int, N>`, `std::vector<long long>`, `std::vector<float>` —
INTEGER_ARRAY / DOUBLE_ARRAY in the table) bound to nothing on either model.

**Fix: the table selects the call.** `node_param_type` moved above the store
calls in `nros/node_parameters.hpp` and the calls are keyed on it: exact
non-template overloads for the three slot types (`bool`, `int64_t`, `double`)
and `const char*`, then ONE `enable_if` template per numeric kind for every
other type the table names. A slot overload is an exact non-template match and
wins for its own type; every other type reaches a template where it is also
exact, so no call is left to conversion ranking and nothing can be ambiguous on
either data model. The `int` overloads are gone — the template covers them.
`nros::tr` (`traits.hpp`) supplies `enable_if`/`is_same`, so `<type_traits>`
is still not needed under `-nostdinc++`.

Conversions are CHECKED where they can lose a value, with the scalar rule
shared by the array forms: an integer read back into a narrower `T` must fit
(`node_param_fits`, a round trip plus a sign test for same-width unsigned), and
an unsigned value above INT64_MAX is refused on declare/set
(`node_param_widens`) rather than wrapped negative. Refusal is
`NROS_CPP_RET_INVALID_ARGUMENT` with `out` untouched (a `Seq` is left
cleared). Floating point narrows by `static_cast`, as rclcpp's does. An array
whose element IS the slot type crosses as before, with no copy; any other
element type converts through a slot-typed scratch buffer.

**Test.** `packages/api/nros-cpp/tests/compile/param_integer_widths.cpp`,
run by `just check cpp` in five arms: hosted LP64 with and without
`NROS_CPP_STD` (the `std::vector` forms), `-nostdinc++` against the ThreadX
shim, and ILP32 through `arm-none-eabi-g++` with and without `NROS_CPP_STD`
(a recorded `nros_check_skip` where that cross is absent, since `clang
--target=armv7a-none-eabi` has no libc for `nros/log.h`'s `<stdio.h>`). It
declares/gets/sets every integer width, `int32_t`/`int64_t`/`uint8_t`,
`float`/`double`, `Seq<T, 4>` and `std::vector<T>` of mixed element types, and
`static_assert`s the range rule. It fails to compile against the old header in
every arm. The existing parameter probes (`ros2_param_launch_seed`,
`param_hosted_overloads`, `param_descriptor_surface`, the `ros2_api_adoption`
family) compile unchanged; `-Wall -Wextra -Wpedantic -Wconversion
-Wsign-conversion` adds no diagnostic in this header.
