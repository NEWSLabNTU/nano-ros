---
id: 2079
title: "`nros new` emits an unmigrated Rust leaf whose edition is a manifest pin, so its `system.toml` cannot choose one"
status: resolved
type: bug
area: [cli, scaffold, examples]
severity: medium
found: 2026-10-26
resolved: 2026-10-27
related: [issue-2045, issue-1305, rfc-0056, rfc-0098, phase-452]
---

# The Rust scaffold pins the ROS edition in its manifest

**Home:** [phase-452](../../roadmap/archived/phase-452-copy-out-front-door.md) W8. Found by
[#2045](2045-example-manifests-hand-set-the-ros-edition-feature.md)'s
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

## Resolution (2026-10-27)

`packages/cli/cargo-nano-ros/src/scaffold.rs` emits the migrated shape for
both reachable Rust variants:

- **linux** (hosted) is now `examples/linux/rust/talker`'s shape: `lib.rs` is
  the node (`talker_node_lib`, shared with the self-bringup), `main.rs` is
  `nros::main!(spin = "forever")`, and `system.toml` names the
  `[[component]]` it boots. The manifest depends on
  `<pkg>_nros_selection = { path = "build/nros/selection" }` and names no
  board crate, RMW backend, RMW feature table or edition.
- **baremetal** (mps2-an385 self-bringup) takes the selection package in its
  target-scoped table instead of the board crate, `nros-rmw-zenoh`,
  `cortex-m`, `cortex-m-rt` and `panic-semihosting`; the board's panic
  provider arrives with it, as in `examples/mps2-an385-baremetal/rust/talker`.
- `system.toml` states `ros_edition` for every language, C/C++ included,
  which the leaf reader has honoured since #2087.

esp32 keeps its manifest pin: it is dormant (issue 1525, not offered by
`nros new`), its example is unmigrated too, and the pin is its only carrier.
For the same reason `check_lowered_edition_features` keeps its migrated-only
scope — an unmigrated cargo leaf still exists.

Measured on scaffolds made by the rebuilt CLI outside the checkout, each
`nros sync` + `nros build`:

| scaffold | build | `nros-node` features |
| --- | --- | --- |
| `--platform linux --ros-edition jazzy` | OK, `build/linux/target/debug/<pkg>` | `ros-jazzy` |
| `--platform linux --ros-edition humble` | OK | `ros-humble` |
| `--platform baremetal --ros-edition jazzy` | OK, `thumbv7m-none-eabi` | `ros-jazzy` |
| the jazzy linux one, `system.toml` edited to humble, sync + build | OK | `ros-humble` |

`book/src/getting-started/first-node-rust.md` describes the new layout.
