---
id: 2590
title: "A Rust component installed by a C++ entry ignores the launch `<node name= namespace=>`"
status: open
type: bug
area: [api, codegen]
severity: low
found: 2026-11-15
related: [phase-478, issue-2568]
---

**Home: [phase-478](../roadmap/phase-478-island-developer-experience-findings.md).**
Found while fixing [issue 2568](archived/2568-rust-ctx-param-misses-launch-values.md).

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
