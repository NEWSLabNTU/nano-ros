---
id: 2638
title: "A list parameter from a launch `<param from>` file reaches the node as one comma-joined string"
status: resolved
type: bug
area: [params, codegen, cli]
severity: high
found: 2026-11-16
resolved: 2026-11-16
resolved_in: "the change that filed it"
related: [phase-478, issue-2598]
---

**Home: [phase-478](../../roadmap/phase-478-island-developer-experience-findings.md), I19.**
Reported by the Autoware Safety Island (phase-15 P3) on nano-ros 5b9e8844a4.

## What happened

The island's controller loads `config/lateral/mpc.param.yaml` through
`<param from>`. Its `steer_rate_lim_dps_list_by_curvature: [40.0, 50.0, 60.0]`
reached the running posix image as the STRING `"40.0,50.0,60.0"`
(`ros2 param get` reported a string), and `record.json` showed the same
string. The island kept every array parameter as a C++ default to work
around it.

## Where it flattened

Not in play_launch. The SystemModel keeps the list (`ParamValue::StrList`,
schema 2). nano-ros baked every resolved value to text with rlm's
`ParamValue::to_bake_string`, which joins a list with `,`
(`plan_record_from_model`, the C/C++ entry lowering in `codegen/entry`, and
`nros::main!`). The image then typed that text by scalar inference
(`infer_param_value` in `nros-cpp` and `nros`), which has no list arm, so
the text became a string. A one-element list became a scalar and `[]` the
empty string.

## Resolution (2026-11-16)

One convention, with a writer and a reader tested as a round trip:

- `nros_orchestration_ir::launch_param_text` writes a list as YAML flow
  text (`[40.0, 50.0, 60.0]`), its elements as the model spells them, and
  single-quotes a string that would otherwise read as something else. Every
  bake site uses it.
- `nros_params::ParameterValue::assign_launch_text` reads it in place:
  scalars infer as before, a quoted text is a string, and a flow sequence is
  a bool, integer, double or string ARRAY, typed by its first element.
  rcl's rule applies to the rest: a list of mixed element types (`[1, 2.5]`)
  is refused, with a log line naming the parameter. `[]` seeds nothing,
  because ROS gives an empty sequence no value; the declaration's default
  stands.
- The C/C++ seed (`nros_cpp_declare_node_param`, `nros_cpp_declare_param`)
  and the Rust seed (`nros::main!`) both read through it.

Test: the features workspace's `cpp_params` image loads
`config/cpp_params.param.yaml` with a double, integer, bool and string list
and an empty one, and `cpp_c_param_live_read_e2e` checks the component reads
each back as an array of its type.
