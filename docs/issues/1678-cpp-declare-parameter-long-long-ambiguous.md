---
id: 1678
title: "C++ `declare_parameter<long long>` does not compile on LP64 hosts: the declare/get overload set has `int64_t` (= `long`) but not `long long`"
status: open
type: bug
area: [api]
severity: low
found: 2026-10-05
related: [1649, phase-446]
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
