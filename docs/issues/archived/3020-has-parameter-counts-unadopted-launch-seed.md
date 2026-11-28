---
id: 3020
title: "`has_parameter()` answers true for a launch seed the node never declared, so `has_parameter ? get : declare` skips the declaration"
status: resolved
resolved: 2026-11-28
resolved_in: "the change that adds this line"
type: bug
severity: medium
area: [params, cpp]
related: [issue-2901, issue-2969, issue-2768]
found: 2026-11-28
---

## What happens

An image's entry seeds every launch parameter of a node before it builds the
node (`codegen/entry/packs/entry/shared/declare_calls.jinja`, through
`nros_cpp_declare_node_param`). The seed takes a store slot marked as a
launch seed, and the node's own first declaration adopts it (issue 2969).

`rclcpp::Node::has_parameter` (`nros_cpp_node_has_param` in
`packages/api/nros-cpp/src/params_shim.rs`) asked only whether the store held
the name. That is true for a seed nobody has declared. rclcpp answers false
there: a launch override is not a declared parameter until the node declares
it (unless the node allows undeclared parameters).

Stock Autoware reads parameters with exactly that distinction in mind:

```cpp
has_parameter(p) ? get_parameter<double>(p) : declare_parameter<double>(p)
```

(`mpc_lateral_controller.cpp:130-134`, `pid_longitudinal_controller.cpp:205-212`,
for `ego_nearest_dist_threshold` and `ego_nearest_yaw_threshold`). On a
nano-ros image both keys are seeded, so the node took the `get_parameter`
branch and never declared them. The slot existed (the seed occupies it, and
sizing counts seeds, issue 2768), but it kept the type inferred from the
launch text instead of the declared `double`. A yaml value written as `1`
would make `get_parameter<double>` fail at boot.

## How it was found

Issue 2901's partial seeding made the metadata probe seed the keys a
component's launch instances agree on, as the image does. The island's
`check-param-seeds.py` (ASI CI run 46098784744, nros12 B50) then read
148 declared / 150 seeded on the controller. The probe's census
(`nros-plan/metadata/controller.json`) had lost exactly these two keys,
because the probed node now took the `get` branch too. Comparing the two pins
(419949e6a5: 190 declared with both keys typed `double`; 95f90e9a7f: 188, both
absent) located it. The probe was faithful: the image had taken the `get`
branch all along, and 2901 only made the probe see it.

## Fix

`ParameterServer::is_declared` answers whether the code declared a name: an
entry that is still an unadopted launch seed does not count. It reaches the
C++ shim through `Executor::has_declared_parameter_on` and the
`ExecutorFfi` trait, and `nros_cpp_node_has_param` asks it instead of
`get_parameter_on(..).is_some()`. `get_parameter` still reads a seed: the
value is in the store either way.

So the idiom declares, and the declaration adopts the seed with the code's
type. A seed whose text reads as another type than the declaration (`1` for a
`double`) is refused with `LaunchValueTypeMismatch`, as rclcpp refuses an
integer override for a double parameter. That behaviour is issue 2969's and is
unchanged.

The refusal names the parameter, both types and the fix on every surface.
For example, `parameter "ego_nearest_yaw_threshold" is declared double but the
launch value is the integer 1; write 1.0 in the YAML`. The C++ header already
printed this (`report_param_value_type_mismatch`). The C (`rclc`) and Rust
declarations now print the same sentence through
`nros_params::LaunchTypeMismatch`, where they used to say only that the types
differed.

Sweep: `nros_cpp_node_has_param` is the only has-parameter query. It backs
`rclcpp::Node`, `rclcpp::NodeParametersInterface` and
`rclcpp_lifecycle::LifecycleNode`. The C surface (`rclc_parameter`) and the
Rust surface expose none.

## Verification

`nros_params::typed::tests::a_launch_seed_is_not_declared_until_the_code_declares_it`
covers four cases:
- a seed is not declared, but its value reads;
- a `double` declaration adopts a `3.0` seed and is then declared;
- a `1` seed under a `double` declaration is refused and stays undeclared;
- a code-declared name is declared.

The test fails with `is_declared` reverted to the old "is it stored" answer.
`a_launch_type_mismatch_names_the_key_both_types_and_the_fix` pins the
refusal's text.

The acceptance witness is the island's own check with issue 2901's partial
seeding relanded: 150 declared / 150 seeded on the freertos-posix job, and
190 declared with both keys typed `double` in the census image.
