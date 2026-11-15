---
id: 2568
title: "A Rust node's `ctx.param()` returns the default, not the launch file's value, and the census reports the read as `param-phantom`"
status: resolved
type: bug
area: [api, cli, census]
severity: medium
found: 2026-11-14
resolved: 2026-11-15
resolved_in: "the change that closed it (the install seam, the macro, the C++ pack and the runtime-crate anchor, one change)"
related: [phase-478, issue-2589, issue-2590]
---

GitHub: NEWSLabNTU/nano-ros#3860

**Home: [phase-478](../../roadmap/phase-478-island-developer-experience-findings.md)**.
Seen on nano-ros 0.6.0-nros8.

## What happens

A Rust node's `ctx.param()` returns the default, not the launch value. Both
`<param from="file.param.yaml"/>` and an inline `<param name=... value=.../>`
fail, although the resolved model shows the value.

The census also reports the read as `param-phantom`; the L4 scaffold waives it
in `system.toml` for now.

## Direction

The launch value reaches `ctx.param()`, and the census sees the read (so the
waiver can go).

## Root cause

`ctx.param(name)` (`NodeContext::param`) reads a slice of launch pairs set on
the context before the component's `register` runs. Which entry installs the
component decides whether that slice is filled:

- **Rust entry (`nros::main!`)**: filled. The generated entry sets
  `runtime.params` from the node's resolved params (inline `<param>` and
  `from=` YAML both lower into the same list) and calls the package's
  `register(runtime)`, which installs through
  `install_node_typed_with_launch_in` with those params.
- **C++ entry, `rust` component arm** (the island's mixed image, every board
  the C++ pack renders, and the PX4 module entry): empty. The arm called
  `__nros_component_<pkg>_install(nullptr, executor, nullptr)`, the C-ABI
  symbol `nros::node!` emits, and that seam had no way to carry params, so it
  installed with none. The entry did seed the same values into the parameter
  STORE first (`nros_cpp_declare_node_param`), which is why the model and the
  ROS parameter services looked right, but `ctx.param` does not read the
  store, and the store exists only when the image has param services.
- **C entry**: no `rust` arm; the C pack refuses a Rust component.

So the defect was one seam on one road, and the inline and `from=` cases are
the same case, since both reach the entry as one lowered list.

## Resolution

The C-ABI install seam carries the params. `nros::node!` now emits
`__nros_component_<pkg>_install(node, executor, self, params, n_params)`,
where `params` is a flat `[name0, value0, name1, value1, ...]` C-string array
(`nullptr, 0` for none). It forwards to
`nros::__macro::install_node_typed_with_c_params_in`, which validates the
array once (non-null, UTF-8; a malformed array returns `-5` before the class
storage is taken) and hands it to the context in place. There is no copy, no
heap and no per-node parameter cap; the array has to live only for the
install call, because `register` runs inside it. `NodeContext` and the
executor sink read both shapes through one internal type, `LaunchParams`.

The C++ pack declares the seam once, in `rust_install_decls.jinja`, included
by the board entry and the PX4 entry. The `rust` arm of `node_body.jinja`
bakes the node's params as a `static const char* const` array and passes it.
The runtime crate's `#[used]` anchor (`cmake/NanoRosRuntimeCrate.cmake`)
names the new signature.

Tests:

- `rust_node_install_carries_its_launch_params` (C++ emit) asserts the baked
  array, the call that passes it, and the prototype. Mutation-checked:
  rendering `nullptr, 0` on the params branch turns it red.
- The `cpp_native_shapes` golden now has a Rust node with params beside one
  without.
- `c_abi_install_delivers_launch_params_to_ctx_param`
  (`packages/rmw/metadata/tests/component_install_launch_params.rs`) calls the
  macro-emitted symbol the way the generated entry does, on a `metadata`
  executor, and asserts what `register` read with `ctx.param`, plus the `-5`
  refusal. Mutation-checked: dropping the params inside
  `install_node_typed_with_c_params_in` turns it red with every value `None`.
- `flat_c_launch_params_*` (`nros` unit tests) cover the array parser.

### The census half is a different cause, and moved to issue 2589

`param-phantom` is not the same defect. The census records parameter
DECLARATIONS (`census_hooks::on_param_declare`, reached from
`declare_parameter*` in every language). `ctx.param(name)` is a read of the
baked launch string, not a declaration, so it records nothing on ANY road,
including `nros::main!`. A contract that lists such a parameter under
`params:` therefore always reads as `param-phantom`, and this fix does not
change that. Closing it needs a recorded read: a census hook in
`NodeContext::param`, and a census rule that a read claims the contract row
without a type to compare (the read carries a string, not a declared type).
It is tracked as [issue 2589](../2589-census-ctx-param-read-unseen.md). Until then the waiver stays, or the component declares the parameter
(`declare_parameter_for_name_with_default`), which the census does see.
