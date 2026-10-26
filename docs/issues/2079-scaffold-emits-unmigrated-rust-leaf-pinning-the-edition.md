---
id: 2079
title: "`nros new` emits an unmigrated Rust leaf whose edition is a manifest pin, so its `system.toml` cannot choose one"
status: open
type: bug
area: [cli, scaffold, examples]
severity: medium
found: 2026-10-26
related: [issue-2045, issue-1305, rfc-0056, rfc-0098, phase-452]
---

# The Rust scaffold pins the ROS edition in its manifest

**Home:** [phase-452](../roadmap/phase-452-copy-out-front-door.md) W8. Found by
[#2045](archived/2045-example-manifests-hand-set-the-ros-edition-feature.md)'s
own refusal, on its first `just ci gate` run.

## Problem

#2045 took the hand-set `ros-<edition>` feature out of every example leaf and
made `nros sync` / `nros build` refuse it, because the selection package
(issue 1305) states the edition the leaf's `system.toml` declares and cargo
unifies the two. Its first run of `check-scaffold-builds` refused the
scaffold:

```
rust_project: FAIL — the emitted project does not build
    Error: …/rust_project/Cargo.toml: `nros` enables `ros-humble`,
    `nros-rmw-zenoh` enables `ros-humble`.
```

`packages/cli/cargo-nano-ros/src/scaffold.rs` (`scaffold_rust_hosted` and
`scaffold_rust_self_bringup`) writes the leaf the way the examples were written
before issue 1305. It names its board crate by hand (`nros-board-linux`), picks
its RMW with `[features] default = ["rmw-<x>"]`, depends on no selection
package, and formats `nros new --ros-edition <e>` into the dependency features
as `"ros-<e>"`. The `system.toml` written beside it states no `ros_edition`.

So a scaffolded project has two answers to "which edition":

- its `Cargo.toml` pin, which is what builds, and
- its `system.toml`, which says nothing, so every reader of the declaration
  (codegen for its message crates, the sizing descriptor) resolves the
  default, humble.

`nros new --ros-edition jazzy` therefore builds a jazzy runtime over a
declaration that reads humble. A user who later adds `ros_edition = "jazzy"`
to `system.toml`, which RFC-0056 says is where it goes, changes nothing in the
build.

#2045's refusal now applies to MIGRATED leaves only, the ones the toolchain
lowers the edition for. For an unmigrated leaf the manifest pin is the only
carrier there is.

## Fix direction

Emit the shape the migrated examples already have
(`examples/linux/rust/talker`, `examples/mps2-an385-baremetal/rust/talker`):

- depend on `<pkg>_nros_selection = { path = "build/nros/selection" }` and name
  no board crate, RMW backend or edition;
- write `--ros-edition` into the scaffold's `system.toml`
  (`[system] ros_edition`), and `--rmw` likewise where it is not already there;
- then drop the migrated-only exemption from `check_lowered_edition_features`
  if no unmigrated cargo leaf is left that needs a pin.

Acceptance: `check-scaffold-builds` builds both Rust variants; a scaffold made
with `--ros-edition jazzy` resolves jazzy for `nros-node` (`cargo tree -e
features -i nros-node`) and for its codegen; editing that `system.toml` to
humble moves both.
