---
id: 2045
title: "41 example manifests hand-set the lowered `ros-humble` feature, so a declared `ros_edition` other than humble cannot build"
status: resolved
type: bug
area: [examples, build]
severity: medium
found: 2026-10-25
resolved: 2026-10-26
related: [rfc-0056, rfc-0031, rfc-0004, phase-452, issue-1305, issue-2087]
---

# Example manifests hand-set the ROS edition as a cargo feature

**Home:** [phase-452](../../roadmap/phase-452-copy-out-front-door.md) W7.

## Problem

The ROS edition is a DECLARED, language-neutral axis: `[image.<id>]
ros_edition` > `[image_defaults]` > `[system]` (RFC-0056 §"Selection +
lowering"), and the toolchain lowers it. For a cargo leaf the lowering is the
generated selection package: since issue 1305 each image's selection names
`ros-<edition>` on the `nros` umbrella and on the board crate. The cargo
feature is the lowering TARGET, never the knob (RFC-0031: a C or C++ user has
no `Cargo.toml` to set it in).

41 example leaves also name the lowered form by hand on their own `nros`
dependency, as `features = [..., "ros-humble", ...]`. Cargo unifies the two
feature sets, so a leaf that declares any other edition enables both, and
`nros` refuses: `packages/api/nros/src/lib.rs:113`.

## Evidence

On a copy of `examples/mps2-an385-baremetal/rust/talker`, tree `4f545011ff`,
with one added line `ros_edition = "jazzy"` under `[system]`:

```
nros sync && nros build mps2-an385-baremetal
error: `ros-{humble,iron,jazzy}` are mutually exclusive — select one ROS edition.
error: could not compile `nros` (lib) due to 1 previous error
```

The generated `build/nros/selection/Cargo.toml:20` carries
`nros = { …, features = ["ros-jazzy"] }`. The leaf's `Cargo.toml:27` carries
`features = ["alloc", "rmw-cffi", "ros-humble", "macros"]`.

So the declaration `system.toml` invites the user to make is the one the
example cannot honour. With the edition left at its default the two agree, so
nothing red shows it.

Reach: `git grep -lE '"ros-(humble|iron|jazzy|kilted|rolling)"' -- 'examples/**/Cargo.toml' ':!*/generated/*'`
gives 41 files: mps2-an385-baremetal 25 (the C and C++ cargo-built images too),
rv-virt-threadx 6, linux 5, px4 3, esp32-c3-baremetal 2.
`packages/testing/**/Cargo.toml` has 41 more (fixtures and bins). They are not
copy-out, but they are the same lowered form, and one is pinned per manifest
rather than per declared image.

## Fix direction

- Drop the edition from every leaf's own `nros` features: the selection
  package states it. Leave capability features (`alloc`, `macros`) where they
  are a property of the node's code rather than an image choice.
- Where a leaf still has no selection package (the "not migrated" leaves
  `leaf_settings.rs` names: RMW / edition still a cargo feature axis), migrate
  it, or have `nros sync` refuse a leaf whose manifest names a lowered axis
  feature, naming the `system.toml` key that owns it. The refusal is the class
  fix; the sweep alone re-grows.
- Acceptance: the copied-out talker above builds with `ros_edition = "jazzy"`,
  and a leaf hand-setting `ros-humble` is refused at `nros sync`.

## Resolution (2026-10-26)

**The pins are gone.** No tracked leaf with a `system.toml` enables a
`ros-<edition>` feature on a dependency any more:

- 30 migrated example leaves (linux 5, mps2-an385-baremetal 25 — Rust, C and
  C++) and 8 migrated `packages/testing` leaves: the edition tokens were
  deleted; their selection package states it, on `nros` and on a direct
  backend dependency.
- The 6 `rv-virt-threadx/rust` leaves are not on the selection road (lib-only
  staticlibs a CMake image links). They now DEFINE `ros-<edition>` in their own
  `[features]` as forwards to `nros` and to whichever backend is enabled
  (Cyclone's sys crate included, which never received an edition before), and
  `nros_threadx_rv64_rust_app` selects `ros-<declared edition>`. That needed the
  CMake road to know the edition at all, which it did not:
  [#2087](2087-standalone-cmake-leaf-ignores-declared-ros-edition.md).
- The 2 dormant esp32 leaves (scaffold tier, issue 1525): deleted. With no
  edition feature `nros-node` behaves as humble, and zenoh's `ros-humble` is
  empty, so the image is unchanged.
- The 3 px4 companions keep theirs: they have no `system.toml`, so there is no
  declaration for the manifest to disagree with.

**The class is refused.** `check_lowered_edition_features`
(`cmd/leaf_settings.rs`) runs in `write_selection`, which `nros sync` reaches
before its first cargo invocation, and in `nros build`. For a leaf that depends
on its selection package it refuses a dependency that enables `ros-<edition>`,
naming the dependency and the `system.toml` that should state it; a
`[features]` forward is accepted. An unmigrated leaf is exempt: nothing lowers
its edition on the cargo road, so the pin is the only carrier it has. The
first `just ci gate` showed why the exemption is needed: `nros new` still
emits such a leaf
([#2079](../2079-scaffold-emits-unmigrated-rust-leaf-pinning-the-edition.md)).
`no_tracked_leaf_pins_the_declared_edition` walks every tracked leaf
(negative control: re-pinning `mps2-an385-baremetal/rust/talker` turns it red).

**Verified** on copies outside the checkout, each built with `nros sync && nros
build`, at the default and with `ros_edition = "jazzy"` under `[system]`:

| leaf | default | jazzy |
| --- | --- | --- |
| `mps2-an385-baremetal/rust/talker` | builds; nros-node + zenoh `ros-humble` | builds; `ros-jazzy` on both |
| `mps2-an385-baremetal/c/talker` | builds; `ros-humble` on both | builds; `ros-jazzy` on both |
| `mps2-an385-baremetal/cpp/talker` | builds; `ros-humble` on both | builds; `ros-jazzy` on both |
| `linux/rust/serial-talker` (xrce) | builds; nros-node `ros-humble` | builds; `ros-jazzy` |

The jazzy mps2 talker is the one this issue failed on.
