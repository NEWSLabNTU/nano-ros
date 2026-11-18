---
id: 2590
title: "A Rust component installed by a C++ entry ignores the launch `<node name= namespace=>`"
status: resolved
type: bug
area: [api, codegen]
severity: low
found: 2026-11-15
resolved: 2026-11-19
resolved_in: "fix(nros, issue 2590): a Rust component in a C++ entry takes the launch name, namespace and remaps"
related: [phase-478, issue-2568]
---

**Home: [phase-478](../../roadmap/phase-478-island-developer-experience-findings.md).**
Found while fixing [issue 2568](2568-rust-ctx-param-misses-launch-values.md).

## What happens

The C++ entry's `rust` arm installs a Rust component through
`__nros_component_<pkg>_install(node, executor, self, params, n_params)`.
Since issue 2568 that seam carries the launch params, but it still carries no
node identity. The Rust node is therefore created under its `NodeOptions`
name, not the launch file's `<node name= namespace=>`. The `nros::main!`
road passes `node_identity` through `install_node_typed_with_launch_in`, so
the same component gets two names on the two roads.

## Direction

Widen the same seam to carry the name and namespace (C strings, null when
the launch names none), and pass them from `node_body.jinja`'s `rust` arm.
Then a mixed image's Rust node is named as the launch says.

## Resolution (2026-11-19)

The seam dropped more than the identity. The Rust road resolves names
against the remaps handed to the install, never the image-wide table
(`nros_cpp_set_launch_remaps`) the C++ entry sets. So a C++ entry's Rust
node also lost every launch remap, silently.

Fixed for both, which is everything the `nros::main!` road carries except
the QoS-override table:
- `__nros_component_<pkg>_install` now also takes `name` and `namespace`
  (C strings, each NULL when the launch states none) and `remaps` /
  `n_remaps`, a flat `[from0, to0, ...]` array.
  - Each identity half stands alone: a launch that names a node without a
    namespace keeps the code's namespace, as the C++ entry's `rclcpp` arm
    treats it. Internally this is `LaunchIdentity`, and `nros::main!`'s
    both-or-neither form converts into it.
- The remaps cross the same way the params do (`LaunchParams::Flat`), and are
  read during the call only.
- `node_body.jinja`'s `rust` arm passes `n.launch_name`,
  `n.launch_namespace` and the node's remaps.

The QoS-override table is still not carried, and codegen still REFUSES a
`rust` component with overrides in a C++ entry (issue 1854), loudly, not
silently. The C entry pack has no `rust` arm.

Tests:
- `nros-rmw-metadata/tests/component_install_launch_identity.rs` calls the
  macro-emitted symbol as the C++ entry does, with name `driver`, namespace
  `/robot` and remap `chatter -> /robot/out`. The node is `driver` in
  `/robot`, the code's name is gone, and the sidecar declares `/robot/out`.
  A null remap target is refused (-5). Mutation, the seam passing no
  identity and no remaps: red, `the image's nodes: ["code_name"]`.
- `tests_cpp.rs::rust_node_install_carries_its_launch_identity_and_remaps`
  checks the emitted array and call. The C++ golden moved by the call shape
  only.

