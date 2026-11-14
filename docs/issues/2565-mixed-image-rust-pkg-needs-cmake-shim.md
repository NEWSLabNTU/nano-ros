---
id: 2565
title: "In a mixed C++ + Rust image the Rust package needs an undocumented CMake shim, and `nros new` does not add it or the contract row"
status: open
type: bug
area: [cli, cmake, book]
severity: medium
found: 2026-11-14
related: [phase-478, 2399]
---

GitHub: NEWSLabNTU/nano-ros#3857

**Home: [phase-478](../roadmap/phase-478-island-developer-experience-findings.md)**.
Seen on nano-ros 0.6.0-nros8 while scaffolding the L4 Safety Island
workspace.

## What happens

In a mixed C++ + Rust image, the Rust package needs:

- a `CMakeLists.txt` calling `nano_ros_node_register(LANGUAGE RUST)`;
- `build_type nros_cmake` in its `package.xml`.

The "Mix languages" book page does not say so. Without them the build fails
with `no matching component in nros-metadata.json`, then a `params:` refusal.

`nros new --lang cpp --to <bringup>` into a workspace that already has a
Rust package also:

- does not add the shim to the existing Rust package;
- does not add the new node's row to the bringup contract.

## Direction

Document the requirement on the "Mix languages" page, and have `nros new`
add the shim and the contract row. Issue 2399 (resolved) fixed the configure
seed for a mixed image; this is the authoring half.
