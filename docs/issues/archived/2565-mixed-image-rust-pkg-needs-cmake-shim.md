---
id: 2565
title: "In a mixed C++ + Rust image the Rust package needs an undocumented CMake shim, and `nros new` does not add it or the contract row"
status: resolved
type: bug
area: [cli, cmake, book]
severity: medium
found: 2026-11-14
resolved: 2026-11-15
resolved_in: "issue 2376\'s fix (2383e29e80) for the contract row; the change that resolved this one for the shim and the docs"
related: [phase-478, 2399]
---

GitHub: NEWSLabNTU/nano-ros#3857

**Home: [phase-478](../../roadmap/phase-478-island-developer-experience-findings.md)**.
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

## Resolution

**The contract row.** Issue 2376's fix (2383e29e80, 2026-11-10) already makes
`nros new --to` add the new node's row to the bringup contract. It landed after
0.6.0-nros8 was cut.

**The shim.** `nros new … --to <bringup>` now writes the CMake registration
into every Rust package of a bringup that mixes languages
(`add_mixed_rust_shims`, `cmd/new_workspace.rs`). That covers a package added
by this call and one that was already there. "Mixes" means the bringup's
`[[component]]` rows name a package with a `CMakeLists.txt` and no
`Cargo.toml`. The registration is a `CMakeLists.txt` calling
`nano_ros_node_register(NAME <row name> CLASS <row class> LANGUAGE RUST
SOURCES Cargo.toml)`, the shape the mixed example carries, plus
`<build_type>nros_cmake</build_type>` in place of `nros_cargo` (a package that
states another build type is refused, naming it). A Rust-only bringup and an
already-registered package are left alone.

**The docs.** The "Mix languages" page states the requirement and shows the
registration. The build's `has no matching component in nros-metadata.json`
refusal now names the Rust registration and the `nros new` that writes it.

Tests (`cmd/new_workspace.rs`):
- `a_mixed_bringup_gives_its_rust_package_the_cmake_registration`
- `a_rust_only_bringup_is_left_alone`

Not done: the generated CMake root still cannot register a cargo-only Rust
package by itself. That would remove the hand-written file altogether, but it
needs a mixed fixture build to verify.
