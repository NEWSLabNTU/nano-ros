---
id: 2554
title: "Tracked `packages/interfaces/generated/humble` crates drift from their emitter (old ids in rustdoc and headers, codegen version 18 vs 21)"
status: resolved
resolved_in: "the change that archives this file"
type: tech-debt
severity: low
area: [codegen, ux, build]
related: [issue-2551, phase-523]
found: 2026-11-14
---

**Home: [phase-523](../../roadmap/phase-523-education-labs-adoption-probes.md) W5.**
Left by [issue 2551](2551-user-surface-text-beyond-function-flow-cites-ids.md),
whose sweep took design ids out of the rustdoc build scripts emit but not out
of this tracked, pre-generated tree.

## What

The six tracked crates under `packages/interfaces/generated/humble` are root
workspace members, and `nros-diagnostics`, `nros-tests` and its bins depend on
them, so the tree is read. It was emitted by an older codegen:

- each crate's `build.rs` header names the codegen step by its old
  spelling (now `nros dev codegen`) with "(phase-403 W6)", names the old
  spelling of `nros configure`, and cites "phase-380's rule";
- each `lib.rs` rustdoc says "(phase-429 W1)" and "RFC-0088's
  `format_check`", and names the old spelling of `nros configure`. That
  rustdoc reaches users of `nros-std-msgs` and the other crates
  (`cargo doc`, IDE hovers);
- `NROS_EMITTED_CODEGEN_VERSION` is 18; the emitter writes 21;
- the bounds JSON lacks `cyclone_schema_shape`.

Nothing checks the tree against its emitter, and `check-user-surface-ids`
does not read it (it is generated output, not emitter source).

## Measured (2026-11-14)

Regenerating with the in-tree CLI, `/opt/ros/humble` and rustfmt gives 324
diff lines over 28 files. Part of that comes from the measuring host's newer
Humble (`ament_version` 1.2.2 → 1.2.3 and 4.9.1 → 4.9.2; nine
`rosgraph_msgs` types instead of one), so a byte-for-byte comparison depends
on the host.

## Plan

1. Regenerate with `just generate-interfaces` on the pinned Humble, decide
   whether the 18 → 21 codegen-version jump is intended, and check the
   consumers accept it.
2. Add a drift check: either regenerate into a staging directory in a lane
   that has ament and diff, or (buildless) check that the tree's
   emitter-owned text (header lines, the codegen-version stamp, `lib.rs`
   rustdoc) matches the emitter templates in `rosidl-codegen` /
   `rosidl-bindgen`.
3. Once regenerated, keep ids out of that rustdoc through the drift check or
   `check-user-surface-ids`' generated-rustdoc reading.

## Resolution (2026-11-18)

**The pin.** The committed tree's ament inputs (`ament_version` 1.2.2 and
4.9.1, one `rosgraph_msgs` type) come from a Humble that is no longer
published. Every Humble available on 2026-11-18 agrees on 1.2.3 and 4.9.2
with nine `rosgraph_msgs` types: `ros:humble-ros-base-jammy` and `ros:humble`
(images built 2026-10, packages synced 2026-09-07), and newslab-118's apt
install (2026-11-05). So the "newer Humble" a host regeneration showed is
simply current Humble, and the committed tree was the outlier. The CI image
(`ghcr.io/newslabntu/nano-ros-ci:humble`, `FROM ros:humble-ros-base`) is
private and could not be pulled here to read its versions; a CI image built
since the 2026-09-07 sync carries the same packages.

**Regenerated** inside `ros:humble-ros-base-jammy`, as the invoking user with
the worktree mounted, by the `just generate-interfaces` steps:
`nros dev codegen rust` with the recipe's six renames, `geometry_msgs`
dropped, and each crate formatted. 26 files changed plus 8 new
`rosgraph_msgs` messages (`Action`, `Graph`, `InterfaceType`, `Node`,
`QoSProfile`, `Service`, `Topic`, `TypeHash`). `NROS_EMITTED_CODEGEN_VERSION`
moved 18 → 21, and the old design ids left the `build.rs` headers and `lib.rs`
rustdoc. The six crates plus `nros-diagnostics` and `nros-node` build with
`--all-targets`.

The six `lib.rs` rows of `.config/api-doc-ids-ratchet.json` fell to 0 and
were lowered. The ratchet itself cannot retire here: 1,035 ids in 183 other
files remain on main, outside this tree.

**Drift gate:** `just check tracked-generated-codegen-version` (fast line).
Every tracked generated crate's `NROS_EMITTED_CODEGEN_VERSION` must equal
`NROS_CODEGEN_VERSION`. That is sufficient because `codegen-version-surface`
already refuses a template change that does not bump the version. Against
main's crates it fails ("emitted at codegen version 18, the emitter writes
21"). It does not compare ament content: that is an input the ROS install
supplies, and the pin above is where it is fixed.
