---
id: 2634
title: "declare_parameter past the parameter store's capacity silently returned the compiled default"
status: resolved
type: bug
area: [api, core, cpp]
severity: high
found: 2026-11-16
resolved: 2026-11-16
resolved_in: "fix(#2634): a declaration the parameter store has no slot for stops the boot"
related: [phase-478, phase-446]
---

**Home:** phase-478 I19 (found by the autoware safety island, GitHub #4018).

## Symptom

`rclcpp::Node::declare_parameter<T>(name, default)` returned `default` on every
declare failure except ALREADY_EXISTS:

```cpp
Result r = ::rclcpp::detail::node_param_declare(h, name, default_value);
if (!r.ok() && r.raw() != RCLC_RET_ALREADY_EXISTS) {
    return default_value;
}
```

When the image-wide store was full (`NROS_MAX_PARAMETERS`, default 32) the
declare failed with FULL and the call returned the compiled default, with no log
and no error. A launch or YAML value for that name was lost, and the node ran
on numbers its launch file does not state. The island's controller declares 149
parameters and hand-counts `max_parameters = 164` with the comment "Too few is
SILENT"; a miscount gives a vehicle controller its default gains.

The read-back after an ALREADY_EXISTS has the same shape: a launch seed whose
inferred type differs from the declared one (`kp: 1` for a double) reads back as
nothing, and the default is returned in silence. That half is NOT fixed here.
`<param from>` seeding currently flattens a YAML list into one comma-joined
string, so a refusal on type would stop every image with an array parameter.
It lands after the flattening is fixed, as its own change (branch
`hold/param-type-mismatch-full` carries it: a boot refusal naming the declared
type, the stored value and what to write, plus a `.breaking` fragment).

## Sweep

`git grep -n "declare_parameter\|node_param_declare\|declare_parameter_on" --
packages/api packages/core/nros-node/src` and every rclc declare entry point:

| Site | Before | After |
| --- | --- | --- |
| `rclcpp.hpp` `Node::declare_parameter<T>(name, default)` | default on any refusal | `detail::refuse_param_declare`: `set_error(PARAM_STORE_FULL_WHAT, NROS_CPP_RET_FULL)` or `PARAM_DECLARE_REFUSED_WHAT` with the FFI code; `Unsupported` (no store built) still answers the default |
| same, read-back after ALREADY_EXISTS | default when the stored type differs | unchanged here; the type refusal follows once list seeding is fixed (see above) |
| `rclcpp.hpp` descriptor, `ParameterValue`, message-descriptor overloads; `rclcpp_lifecycle` bound forwards; `NodeParametersInterface` | forward to the above | inherit the fix |
| `node.hpp` `declare_parameters(prefix, map)` | returned the `Result` only | returns it and latches `set_error` |
| `params_shim.rs` `declare_on_node` (every `nros_cpp_node_declare_param_*`) | `FULL` for any refusal, no log | `report_parameter_refused`; `FULL` only for a full store, `ERROR` otherwise |
| `params_shim.rs` `nros_cpp_declare_param` / `nros_cpp_declare_node_param` (launch seeds) | `ERROR`, no log | reported; `FULL` for a full store |
| `nros-c` `parameter.rs` `declare` (`rclc_add_parameter`, `rclc_parameter_declare_*`) | `RCL_RET_ERROR`, no log, for a full store, a quota and a type clash | full store: `RCLC_RET_FULL` and the report; quota and type clash: `RCL_RET_ERROR` (rclc's) and a log line |
| `nros` `node_runtime.rs` `seed_launch_params` | `Err(Runtime)`, a line with no knob | `Err(ParameterRejected)`, `report_parameter_refused` |
| `nros` `node_runtime.rs` declared parameter | `Err(ParameterRejected)`, a line with no knob | same `Err`, `report_parameter_refused` |
| `nros` `NodeDeclError::ParameterRejected::message` | "store is full, or rejected" | names the knob |
| `nros-params` typed builder (`mandatory` / `optional` / `read_only`) | `Err(DeclarationError::StorageFull)` | unchanged `Err`; `DeclarationError` now `Display`s, and `StorageFull` names the knob |
| `nros-node` `Node::declare_parameter_with_descriptor` | `false` | unchanged (already a refusal, not a default) |

Not changed, on purpose: `get_parameter<T>(name)` / `get_parameter_or` answer
a default or `T()` for a missing name, as upstream's `get_parameter_or` is
meant to; after this fix a missing name there follows a declaration that
already latched the boot. `rclcpp_lifecycle::LifecycleEngine`'s UNBOUND form
(no node) returns the default; it has no node to latch.

## Fix

`Executor::report_parameter_refused(node, name)` (nros-node, reached by every
language through `ExecutorFfi`) logs at ERROR and returns whether the store was
full. A full store is THREE records, because one long line was cut at the
128-256 byte log buffer exactly where the knob's name is (measured: the first
version of the test failed on that). The second and third are
`nros_params::STORE_SIZE_KNOB` and `STORE_SIZE_DERIVED`. From `just check cpp`:

```
[ERROR] nros: node "/full_node": parameter "p32" not declared: the parameter store is full (32 slots, all nodes). Its launch/YAML value is not applied.
[ERROR] nros: Raise `max_parameters` under [knobs.params] in system.toml, NROS_MAX_PARAMETERS or CONFIG_NROS_MAX_PARAMETERS.
[ERROR] nros: Or state none: the build derives it when every node declares its parameters under `params:` in its contract.
[ERROR] node "full_node": FAILED at declare_parameter: parameter store full; raise `max_parameters` under [knobs.params] (NROS_MAX_PARAMETERS / CONFIG_NROS_MAX_PARAMETERS) (code=-6)
```

`DeclarationError::StorageFull`'s `Display` is `STORE_SIZE_REMEDY`, the two
joined.

## How the store is sized

`nros-params/build.rs`: `knob("NROS_MAX_PARAMETERS", board rung, descriptor
`[params] max_parameters`, 32)` -- env, then Kconfig, then the board/image
`[knobs.params]` rung, then the contract-derived descriptor value, then 32.

The derived value is `ParamDeclarations::sizing` (`nros-cli-core`
`entity_inventory.rs`): every node's declared `params:` names plus one
`use_sim_time` per node, summed over the image. It comes from the SystemModel's
`contracts.node_params` only, never from the census. It reaches the build on
every road that writes a sizing descriptor: a single-package cargo leaf, a cargo
workspace image, the cmake model road, a Zephyr west entry, and a multi-entry
configure's shared runtime (the union over its images, `union_over_images`).
Zephyr also forwards it as the derivable knob. It is NOT derived when:

- a rung states a number (that number wins, too small or not);
- the contract declares no `params:` (default 32);
- only some nodes declare `params:` (the build refuses, issue 1421);
- the image is unwired, or a standalone cmake leaf without a model (no
  declarations reach the descriptor).

With a contract that declares every parameter, the C++ `check_declared_param`
already refuses a name the contract lacks, so a derived store cannot fill from
code; FULL then means a stated number below the need. Deriving from the census
would mean counting the census's recorded `(node, name)` declarations
(`census_hooks::on_param_declare`) into `ParamDeclarations` for roads with no
contract `params:`; not done here.

## Tests

- `nros-node` `a_full_store_reports_the_refusal_and_names_its_knob` and
  `a_full_store_error_names_its_knob`.
- `just check cpp`: `param_store_full_runtime.cpp` fills the `param-services`
  archive's store and asserts `!ok()`, `NROS_CPP_RET_FULL`, the latched text and
  the logged line.
- `just check c`: `rclc_parameter_server.c` fills the store through a node whose
  quota is the store's capacity and asserts `RCLC_RET_FULL` and the line.
