---
id: 793
title: "C ships two disjoint parameter stores — parameters declared on the
  node-local one are invisible to `ros2 param`, and its accept/reject callback
  fires for nobody"
status: open
type: bug
area: api, params
related: [rfc-0036, phase-379, issue-0788]
---

## Problem

`packages/api/nros-c/include/nros/parameter.h` exposes two parameter stores that
never meet.

**The node-local one** — `nros_param_server_t`, with
`nros_param_server_init`, `nros_param_declare_bool`/`_integer`/`_double`/
`_string` and their `_array` forms, `nros_param_get_type`, and
`nros_param_server_set_callback`. The header's own text calls this "the legacy
API".

**The executor-owned one** — `nros_executor_declare_param_*`, read by the six
`rcl_interfaces/srv/*` servers that `nros_executor_register_parameter_services`
starts. The header says so at line 393: *"parameters declared via
`nros_executor_declare_param_*()` are visible to `ros2 param` tooling."*

Nothing joins them. A C user who declares parameters on `nros_param_server_t` —
the API named `parameter.h`, with the fuller type coverage — gets a node whose
parameters `ros2 param list` cannot see and `ros2 param get` cannot read.

Three consequences, each verifiable:

1. **`nros_param_server_set_callback` fires for nobody.** It is the accept/reject
   veto — a `bool` return, the same contract as rclcpp's
   `add_on_set_parameters_callback` — and it is installed on the store that no
   service reads. A remote `ros2 param set` never reaches it.
2. **Array parameters are unreachable from `ros2 param`.**
   `nros_param_declare_*_array` covers all ten `rcl_interfaces` types;
   `nros_executor_declare_param_*` covers only the four scalars. Anything an
   array parameter is for cannot be exposed.
3. **Descriptors, ranges, read-only flags and `list_parameters` exist only in
   Rust**, so `~/describe_parameters` answers empty descriptors for everything a
   C user declared, and `~/set_parameters_atomically` is served over the wire and
   callable from no language at all.

## RFC-0036 is wrong here in both directions

Line 91 says "**No parameter callbacks**; parameters are read/write only".

* C **does** ship a callback — `nros_param_server_set_callback` — so the RFC
  understates C.
* C++ and Rust have **none** (`grep -c callback packages/core/nros-params/src/server.rs`
  → 0), so a reader who believes the RFC applies it uniformly is wrong about
  which language has what.

This is the third RFC-0036 line phase 379 has found stale (see issue 0783 for the
Errors row and issue 0792 for the lifecycle line). The pattern is the reason the
campaign built a checker rather than another prose catalog.

## Our three languages also disagree about parameters

* **A C++ node cannot set a parameter at all.** `nros::ComponentNode` carries
  rclcpp's exact `declare_parameter<T>` / `get_parameter<T>` / `has_parameter`
  and no setter, typed or otherwise. C and Rust both have full setter families.
* **`component_node.hpp` is not included by `nros/nros.hpp`.** So through the
  umbrella header a ported node gets `nros::Node` (no parameter method at all)
  plus a standalone `ParameterServer<Cap>` the node cannot see — the rclcpp-shaped
  surface exists and only the generated entry point reaches it.
* **`get_type` exists in C and Rust and not in C++**; `nros::ParameterType` is
  named in C and Rust and in no C++ header.
* **Undeclare exists only in Rust** (`ParameterServer::remove` / `unset`).
* **C uses both words in one header**: the functions say `param`
  (`nros_param_declare_bool`) and the types they take say `parameter`
  (`nros_parameter_t`), so `nros_param_get_type()` returns an
  `nros_parameter_type_t`. Belongs with issue 0788.
* **Rust ships two names for one enum** — `SetParameterResult` and
  `ParameterError` name the same five failures, with a `From` impl that `panic!`s
  on the two cases it does not cover, and the inherent setters return one while
  the typed wrappers return the other.

## Also found, and FIXED 2026-08-25: two copies of one QoS profile, disagreeing

`QOS_PROFILE_PARAMETERS` had no caller — `register_parameter_services` passed
`QosSettings::services_default()` — so every parameter server ran on a depth-10
queue where ROS 2 gives them 1000. That matters exactly when a tool sets many
parameters at once, which is what the deep queue is for. Now wired to
`parameters_default()`.

**And this issue was wrong about why.** It said the profile was transient-local
"so a late-joining tool still sees declared parameters". Upstream
`rmw_qos_profile_parameters` is KEEP_LAST(1000) + RELIABLE + **VOLATILE**
(verified in `/opt/ros/<distro>/include/rmw/rmw/qos_profiles.h`). We had **two
copies** of the profile that disagreed: `nros::qos::PARAMETERS` (correct,
volatile) and `QosSettings::QOS_PROFILE_PARAMETERS` (transient-local, wrong) —
and the one named after the upstream constant was the wrong one.

Worse, `test_qos_profile_parameters` **asserted the wrong value**, so the
disagreement between our two copies survived every test run. A test that pins a
defect is the reason it lasted. Both are corrected.

## Evidence

`packages/api/nros-c/include/nros/parameter.h` (both families, and line 393's
claim), `packages/core/nros-params/src/server.rs`,
`packages/api/nros-cpp/include/nros/component_node.hpp`,
`scripts/api-parity.py --topic param`, and
`docs/reference/api-parity-ledger/param.json` — 32 gaps, 22 renames.

## Direction

Not decided here. The first question is whether the legacy store should be
**deleted** rather than joined: it duplicates a capability the executor-owned
store has, it is the one a `parameter.h` reader reaches first, and every
consequence above follows from it existing. If it stays, it needs to feed the
services, and its callback needs to be on the path a remote set actually takes.
