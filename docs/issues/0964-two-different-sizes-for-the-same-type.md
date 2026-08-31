---
id: 964
title: "The C++ header states an ESTIMATED size for every type, including types that have no bound"
status: open
area: codegen
severity: medium
related: [0896, 0939, 0940, phase-403, phase-380]
---

# One type, two numbers, and the wrong one is the one in the header

## What was measured

`rosidl-codegen`'s C++ pack computes `SERIALIZED_SIZE_MAX` with
`compute_serialized_size_max`, which ESTIMATES: a flat 512 per nested message, a
flat default capacity per string, and it ALWAYS returns a value. It therefore
cannot report "unbounded".

Over 120 types in 12 stock ROS Humble packages:

* **81 of 120 have no derived bound, and the C++ header states a size for every
  one of them.**
* Of the 39 that are bounded, the estimate matched the derived bound **zero
  times**: 38 over, 1 under. `geometry_msgs/Twist` reads 1028 against a derived
  64.

On the island entry the same divergence, per type:

| type | C++ header | derived |
| --- | ---: | ---: |
| `autoware_control_msgs/Control` | 2052 | 114 |
| `nav_msgs/Odometry` | 1804 | unbounded, until a cap |
| `autoware_vehicle_msgs/SteeringReport` | 527 | 24 |
| `autoware_adapi_v1_msgs/OperationModeState` | 572 | 27 |

## Why it matters

phase-403 W6 exports the DERIVED bound. The C++ header keeps emitting the
ESTIMATE. The same type now carries two numbers, and the estimate is the one a
user reads, since it is the constant the header advertises and the one
`{Msg}_RX_MAX_SERIALIZED_SIZE` names.

It also violates phase-380's rule directly: a number nobody chose, substituted
where the honest answer is "no bound exists". That rule is why an unbounded type
is a build error at all, and this path quietly opts out of it.

Real cost, not hypothetical: the island's sizing was planned against the
estimate, so the receive buffer, arena and payload classes were all budgeted for
`Control` at 2052 bytes when it serializes to 114.

## Options

1. **Delete the estimator** and emit the derived bound, poisoning the constant
   for an unbounded type exactly as the C pack already does
   (`unbounded_token` + `unbounded_reason`). Behaviour change: a type that is
   unbounded stops having a size constant, which is the point.
2. **Keep both, renamed.** The estimate becomes something honestly named
   (`..._ESTIMATED_SIZE`) and the derived bound takes the load-bearing name.
   Cheaper, but leaves a number nobody chose in the header.
3. **Emit the derived bound only where one exists** and nothing otherwise,
   which is (1) without the poison token.

(1) matches what the C pack does today and what phase-380 requires. It is a
behaviour change with a blast radius across every C++ consumer, which is why it
is filed rather than done.

## LANDED -- option 1, in codegen

`SERIALIZED_SIZE_MAX` is now `nros_serdes::size::max_serialized_size`, for
messages, service request/response and action goal/result/feedback alike. The C
pack and the C++ pack derive it through ONE funnel
(`generator::common::derive_header_bound` -> `bounds::header_bound`), so the two
headers and the exported inventory cannot disagree about whether a bound exists,
what it is, or what the hole is called when there is none.

Re-measured over the same 12 stock packages (126 types by this crate's parser,
not the 120 the filing quotes; the phase-403 W7 table already says 126):

| | before | after |
| --- | ---: | ---: |
| types stating a size | 126 | 40 |
| types with NO derived bound | 86 | 86, and each now states NO size |
| bounded types where the stated number equals the derived bound | 0 of 40 | **40 of 40** |

Reference types, C++ header before -> after:

| type | before | after |
| --- | ---: | ---: |
| `autoware_control_msgs/Control` | 2052 | 114 |
| `autoware_vehicle_msgs/SteeringReport` | 527 | 24 |
| `autoware_adapi_v1_msgs/OperationModeState` | 572 | 27 |
| `nav_msgs/Odometry` | 1804 | none -- unbounded |
| `nav_msgs/Odometry`, with `std_msgs/Header.frame_id` and `Odometry.child_frame_id` capped at 64 | 1612 | 880 |

**The poison has a different SHAPE in C++, because C++ cannot express the C
one.** The C pack writes `#define X <undeclared identifier>`, which costs
nothing to include and errors when NAMED. A C++ `static constexpr size_t X =
TOKEN;` errors at the point of DEFINITION, so every consumer of the header would
break rather than only the consumers asking for a size. The C++ form is a static
data member of an INCOMPLETE type -- `[class.static.data]` permits a
non-defining static-member declaration to have incomplete type -- which keeps the
same two properties: free to include, an error to name, and the error prints the
identifier, which carries the type and the member. Both halves are compiler-
checked in `tests/cpp_bound_compile_check.rs` (g++ and clang++ both name the
token). The identifier is BYTE-IDENTICAL to the C pack's for the same type.

**The estimator survives for exactly one job, renamed to say so.**
`types::storage_serialized_size` sizes the FFI publish glue's stack buffer for a
type that HAS no bound. That is a question about the generated C++ struct's fixed
inline storage, not about the ROS type, so it is not a fabricated bound -- but it
is also not a bound, and it is emitted into no header, exported on no transport
and reachable by no user. A BOUNDED type's publish buffer is now the derived
number, so the flat-512 under-count is gone from every type that has a bound.

## NOT resolved -- in-tree C++ consumers of an unbounded type

The rule now bites, and 15 in-tree example files name `SERIALIZED_SIZE_MAX` on a
type that has no bound. Exactly two types are involved:

* `std_msgs/String` (`string data`) via `Subscription<M>::try_recv` --
  `examples/{native,qemu-arm-freertos,qemu-arm-nuttx,qemu-riscv64-threadx,threadx-linux}/cpp/listener/src/main.cpp`
* `example_interfaces/Fibonacci` `Result` + `Feedback` (`int32[] sequence`) via
  `ActionClient::get_result`, `Stream::try_next`, `ActionServer::publish_feedback`
  and `complete_goal` --
  `examples/{native,qemu-arm-freertos,qemu-arm-nuttx,qemu-riscv64-threadx,threadx-linux}/cpp/action-{client,server}/src/main.cpp`,
  plus `examples/workspaces/cpp/src/action_server_pkg/src/FibServer.cpp`

Everything else in the C++ instantiation set is `std_msgs/Int32`,
`example_interfaces/AddTwoInts`, `Fibonacci::Goal` or `custom_msgs/Reading`, all
bounded. The `bind_subscription_raw` / `Node::create_subscription` /
`bind_service` / `Publisher::publish` / `try_recv_raw` paths do not name the
constant at all, so nothing reaching them is affected.

**Why a `cap` does not fix it today, which is the finding.** Both types are
CONSUMED, not defined in the example: they come from `/opt/ros/humble/share` or
the bundled `packages/cli/interfaces/`, so there is no `.msg` to bound. The
remedy would be a `cap` in `nros-codegen.toml` -- and there is no way to point
codegen at one for a consumed package:

* `nros_generate_interfaces()` takes `CODEGEN_CONFIG`, but it is the call a
  package that DEFINES its interfaces makes. No in-tree caller passes it.
* These examples reach codegen through `nano_ros_add_executable` ->
  `_nros_generate_declared_interfaces` -> `nros_find_interfaces()`, and
  `nros_find_interfaces()` accepts no `CODEGEN_CONFIG` and forwards none.
* The fallback, `CapacityResolver::discover`, walks UP from the codegen OUTPUT
  dir, which is under the CMake binary tree. Whether it ever reaches the example
  source leaf depends on where the fixture build put the build dir, so a
  `nros-codegen.toml` next to the example is not reliably found.

So bounding these is a cmake change (thread `CODEGEN_CONFIG` through
`nros_find_interfaces`, or anchor discovery at the consuming package's SOURCE
dir) plus a config file per affected leaf -- and it is a decision, not a patch:
the config is per-package and not per-language, so capping `std_msgs/String` for
the C++ listener also changes the C pack's struct for every build that sees the
config. Deliberately NOT done here, per phase-380: the alternative was to
reintroduce a default, which is the thing this issue is about.

The C++ API also has no sibling for the C pack's `{Msg}_subscribe_sized`, which
is the other legitimate answer for a type with no bound ("the caller picks the
number"). `Subscription<M>::try_recv_raw` is the raw shape of it; a typed
caller-sized `try_recv` does not exist. phase-403 names this as "the C++ sibling
of this work".
