---
rfc: 0121
title: "Workspace-first project layout: every project is a colcon workspace, `system.toml` lives only in a bringup package, and a single node is a scaffolded bringup plus `nros run <node>`"
status: Draft
since: 2026-11
last-reviewed: 2026-11
implements-tracked-by: [phase-531]
amends: [0098, 0116, 0026, 0066, 0120]
supersedes: []
superseded-by: null
---

# RFC-0121 — Workspace-first project layout

**Status:** Draft (2026-11-03). Written from the maintainer discussion of
2026-11-03. Implemented by
[phase-531](../roadmap/phase-531-workspace-first-project-layout.md).

## Summary

Every nano-ros project is a colcon-shaped workspace, `<ws>/src/<package>/`,
including a project with one node. A `system.toml` lives **only in a bringup
package**. It is the project layer (RFC-0103 D1) and never sits beside a
package's code. A bringup with no `launch/` directory gets its launch file
generated from its `[[component]]` rows, so a bringup may consist of
`package.xml` and `system.toml` and nothing else. That one generation step is
the only place the two project sizes differ; everything after it is the
workspace road.

The single-node shortcut is then two things, and neither is a second road:

- `nros new <ws> --board <b> --template <t>` writes the workspace, the node
  package and a minimal bringup, so the reader authors nothing to get output;
- `nros run <node>` and `nros flash <node>` resolve a node name to the one
  bringup whose only component it is, with that bringup's only image.

The standalone package ("leaf") shape retires, and with it the code that exists
only for it: the per-leaf selection crate, the `--from-leaf` sizing road, the
leaf-only halves of `leaf_settings` / `leaf_system` / `leaf_kconfig`, the two
older self-bringup synthesisers, and the plain `cargo build` flow RFC-0098 kept.

## Problem

### Two project shapes, one of them carrying its own road

A workspace has node packages, bringup packages and board packages. A
standalone package puts all of it in one directory: code, `package.xml`, a
cargo or CMake root, and a `system.toml` that selects a board. Measured on
2026-11-03:

- 223 tracked `system.toml` files sit beside a `Cargo.toml` or `CMakeLists.txt`
  (examples, fixtures, test bins). 39 sit in bringup-only directories.
- Three mechanisms invent a bringup for a package that has none:
  `synthesise_leaf_launch` (`cmd/ws.rs`, from `[[component]]`),
  `synthesise_self_bringup` (`orchestration/nros_config.rs`, from `[deploy.*]`
  manifest metadata no example uses) and `synthesise_self_model`
  (`orchestration/launch_synth.rs`, an in-memory one-node model).
- The standalone package is its own cargo root, so it needs what a generated
  entry already has: a generated `<pkg>_nros_selection` crate to reach its
  board (RFC-0098 amendment, 2026-10-11), its own sizing producer
  (`--from-leaf`, RFC-0100), its own descriptor writer (`write_for_leaf`), and
  shape checks across the build (39 in `cmd/build.rs` alone).

The cost is not the launch file. It is that a standalone package is built by a
different road, and every later feature had to be written for both.

### The conflict this RFC removes

RFC-0103 D1 puts every selection (board, RMW, network) in the project layer and
says the package layer never states one. A standalone package's `system.toml`
is a project-layer file in a package directory, so "where does `system.toml`
live" has two answers: in a bringup, or in a node package. Every reader of it,
and every gate about it, has to handle both.

### Why not "always write a launch file"

RFC-0116 D6 asks that the first chapter names no launch file, bringup, image or
entry. A mandatory hand-written launch file and bringup package for one node is
the concept cost D6 exists to avoid, and it does not remove the separate build
road either.

## Design

### D1 — Every project is a workspace

A project is a directory with `src/`, holding packages, the shape colcon and
`ros2 pkg create` already use. `nros build`, `nros run` and `nros flash` run
from the workspace root. There is no standalone package shape: a directory with
code and a `system.toml` is refused (D7).

Package kinds, each recognised by what it holds:

| kind | holds | recognised by |
| --- | --- | --- |
| node | nodes (components) | `package.xml` with code; no `system.toml`; no selection in its manifests (RFC-0103 D1 layer 4) |
| bringup | system, images, components, optional `launch/`, optional `*.contract.yaml` | `system.toml` |
| board | board facts, run/flash/console tables | `<nano_ros_provides kind="board">` and `nros-board.toml` (RFC-0087 D3, RFC-0118 D4) |
| runner / command | host tools | `<nano_ros_provides kind="runner\|command">` (RFC-0119) |
| message | interfaces | `rosidl` package |

A node package is a library. Its `main` is generated with the image's entry, as
a workspace node's already is. A node package's `Cargo.toml` / `CMakeLists.txt`
names no board, no RMW and no selection crate.

### D2 — `system.toml` lives only in a bringup

A bringup package is `package.xml` plus `system.toml`, and nothing else is
required. Its schema is unchanged: `[system]`, `[[component]]`, `[image.<id>]`,
`[host.*]`, `[tiers.*]` and the knob tables. A workspace may hold several
bringups, as it does today.

A fact about the WORKSPACE, not about one system, does not go in a bringup. Two
bringups could disagree about it, and nothing could decide between them. Such
facts live in the workspace root's `nros.toml`: the package search path
(RFC-0087 D6, `[workspace] package_paths`) and the declared provider overrides
(RFC-0119 D4 as amended, `[workspace.overrides]`). The overrides' fallback to a
root `system.toml` when there is no `nros.toml` exists only for the standalone
shape, and retires with it (phase-531 W7).

### D3 — A missing launch file is generated, in every bringup

If a bringup has no `launch/` directory, `nros configure` (RFC-0120 D2; today
`nros sync`) writes `<ws>/build/nros/launch/<bringup>/system.launch.xml` from its
`[[component]]` rows and resolves it as if it were authored. This is the one
generation step that differs by project size; the resolver, the model, the
entry generator, sizing and the run plan see an authored and a generated launch
file identically.

- `nros plan explain <bringup>` shows the generated file and says it was
  generated.
- Adding `launch/system.launch.xml` replaces the generated file. The scaffold
  for a second node (D5) writes the generated content out, so the move changes
  no behaviour. Deleting `launch/` goes back to generation.
- A bringup with neither `launch/` nor a `[[component]]` row is refused, naming
  both remedies.

`synthesise_leaf_launch` is that step, moved to read bringups. The other two
synthesisers are deleted.

### D4 — The scaffold writes the bringup

```bash
nros new sensor_ws --board mps2-an385-baremetal --template talker
```

writes:

```text
sensor_ws/
└── src/
    ├── sensor_node/            # node package: package.xml, Cargo.toml (a library), src/lib.rs
    └── sensor_node_bringup/    # package.xml and system.toml; no launch/
```

The bringup's `system.toml` names the board, the RMW, the one component and one
image. The reader never has to open it to get output. `nros new` inside an
existing workspace adds a package and, with `--to <bringup>`, a `[[component]]`
row.

### D5 — `nros run <node>` and `nros flash <node>`

The target argument of `run`, `flash` and `monitor` (RFC-0118 D2) accepts, in
this order:

1. `<bringup>:<image>`, the full spelling;
2. `<image>` when exactly one bringup declares it;
3. `<node>`: the one bringup whose `[[component]]` rows name only that node's
   package, with that bringup's only image;
4. nothing: the workspace's only image.

Any other case refuses and lists the `<bringup>:<image>` spellings that would
resolve, so a shortcut never picks one candidate out of several. `--why` names
the rung that resolved. The same resolver serves `nros build --packages-up-to`
when it is given a node name.

### D6 — One build road

Every image, of any project size, builds through the workspace builder's
generated entry (RFC-0065, RFC-0098 D6). This reverses RFC-0098's 2026-10-11
"Rejected: making every single-package leaf a generated entry": the plain
`cargo build --manifest-path <leaf>/Cargo.toml` flow it kept is retired as a
supported user flow. `nros build` is the front door (RFC-0116, RFC-0120 D1
layer 2). The hooked host-tool roads of RFC-0120 D8 are kept, against the
generated entry, which records its bringup so the hook needs no directory walk.

Out of scope, unchanged: projects that own their build and have no
`system.toml`. That means a guest Zephyr application (RFC-0085 D14), a CMake
`add_subdirectory` consumer, and a vendor-SDK project (RFC-0064).

### D7 — Refusals and gates

- `nros configure` and `nros build` refuse a `system.toml` in a directory that
  also holds `Cargo.toml` or `CMakeLists.txt`. The message names the move
  (`nros dev migrate-bringup <dir>`, D8) and this RFC.
- `check-system-toml-in-bringup` (fast line) refuses a tracked `system.toml`
  beside code. It starts as a ratchet over the 223 measured sites and ends at an
  empty allowlist; there is no permanent exception.
- `check-node-package-selection-free` refuses a selection crate or a board
  crate named in a node package's manifest.

### D8 — Migration

A one-shot `nros dev migrate-bringup <dir>` turns a standalone package into a
workspace. It is kept in `scripts/oneshot/` once the tree is migrated.

- `<dir>/` becomes `<dir>/src/<pkg>/` (the node) plus `<dir>/src/<pkg>_bringup/`,
  holding `package.xml` and the moved `system.toml`.
- A hand-written `main` that only calls `nros::main!` is deleted; any other
  `main` is reported for a person to move.
- The selection-crate dependency is removed from the manifest.
- `fixtures.toml` rows are rewritten to the new paths, keeping each row's
  coordinate (`row_coord()` is unchanged), so lane membership does not move.

Examples stay copy-out projects (RFC-0026). What a reader copies is now a
workspace, `examples/<plat>/<lang>/<example>/` with `src/` inside.

## Amendments to other RFCs

| RFC | Amendment |
| --- | --- |
| 0098 | The 2026-10-11 "Rejected: making every single-package leaf a generated entry" is reversed (D6). The selection-package section retires with the shape |
| 0116 D6 | The first chapter still names no launch file, image or entry. It now shows a workspace holding two packages, both written by the scaffold (D4). "Grows into a workspace by `nros new --workspace`" becomes "grows by adding a package" |
| 0026, 0066 | An example is a copy-out WORKSPACE. A feature is still a node package and a configuration is still a fixture axis (RFC-0066); a single-package example is one node package plus one bringup |
| 0120 | "The project is the leaf directory or the workspace root" becomes "the workspace root". D6 case 1 (plain `west build` of a leaf) becomes a plain `west build` of the generated entry. D8's rust-analyzer include moves to the generated entry |
| 0100 | The `--from-leaf` producer and `write_for_leaf` retire; every image takes `write_for_model` |
| 0118 D2 | The target argument gains the node rung (D5) |

## Retirements

| What | Where | Replaced by |
| --- | --- | --- |
| standalone package shape | 223 `system.toml` beside code | node package plus bringup (D8) |
| `<pkg>_nros_selection` crate | `leaf_system::selection_crate_name`, `LEAF_SELECTION_DIR` | the generated entry names the board |
| `synthesise_self_bringup`, `[deploy.*]` manifest metadata | `orchestration/nros_config.rs` | D3 |
| `synthesise_self_model` | `orchestration/launch_synth.rs` | D3 |
| `--from-leaf`, `write_for_leaf` | sizing descriptor, `cmake/NanoRosSizingDescriptor.cmake` | `write_for_model` |
| leaf-only paths in `leaf_settings`, `leaf_system`, `leaf_kconfig`, `leaf_entity_env`, `west_leaf_sizing`, `build.rs` | `nros-cli-core` | the workspace road |
| plain `cargo build` of a package | RFC-0098 | `nros build`; a hooked host tool on the generated entry |
| a "workspace package that is itself a leaf" | copy-out templates' Rust packages | a node package |
| `[overrides]` read from a root `system.toml` | RFC-0119 D4's fallback for a project with no `nros.toml` | `nros.toml [workspace.overrides]` |

Phase-531 W1 measures what each row deletes before anything is removed, because
some `leaf_*` code also serves workspace entries.

## Alternatives considered

- **`system.toml` allowed in the node package (the first sketch of
  2026-11-03).** No extra directory, but the project-layer file then has two
  homes, and the standalone package keeps either its own road or a desugaring
  step that every reader has to know about.
- **A workspace-root `system.toml` for the shortcut.** Avoids the bringup
  directory, but moves the same conflict to the root, and needs a precedence
  rule between the root file and the bringups that RFC-0103's one-owner rule
  forbids.
- **Always a hand-written launch file and bringup.** The most uniform, and the
  concept cost RFC-0116 D6 measured against. D3 keeps the uniformity without the
  cost.
- **Selections on the command line (`nros run sensor_node --board X`).**
  RFC-0103 D2 makes the command line an override, never a source.

## Open questions

- Whether `nros new <ws>` names the bringup `<node>_bringup` or `bringup`. The
  former survives a second node without a rename; the latter reads better in the
  first chapter.
- How rust-analyzer finds a node package's settings when the cargo root is the
  generated entry (RFC-0120 D8 left the include for a later phase).

## Changelog

- 2026-11-03: Draft, from the maintainer discussion of 2026-11-03 (option C:
  a scaffolded bringup and a node-name shortcut, over a node-package or
  workspace-root `system.toml`).
