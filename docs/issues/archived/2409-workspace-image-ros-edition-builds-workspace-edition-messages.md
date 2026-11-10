---
id: 2409
title: "A workspace image that declares its own `ros_edition` builds with the workspace's edition of the messages, and nothing says so"
status: resolved
type: bug
severity: medium
area: [cli, codegen, build, interop]
related: [phase-531, rfc-0121, rfc-0056, 2260]
found: 2026-11-10
resolved_in: "the issue-2409 pull request: `workspace_edition_mismatch` refuses the image in `plan_for`; the linux Rust talker's edition images build from a per-edition copy of its workspace"
---

**Home: [phase-531](../../roadmap/phase-531-workspace-first-project-layout.md) W6.**
It blocks migrating `examples/linux/rust/talker`, whose six edition images
(`linux_iron`, `linux_jazzy`, and their `_xrce` / `_cyclonedds` twins) are what
`just ros_editions build-e2e-fixtures` builds.

## What

A single-package leaf could carry one image per ROS edition: `nros configure
--image <id>` regenerated the leaf's `generated/` for that image's edition
(`resolve_sync_edition`, the `leaf_image` arm), and `nros build` refused a
`generated/` of another edition (`cmd/build.rs`, the issue 1305 check).

On the workspace road neither holds. `resolve_sync_edition` documents the
reason: "A workspace (`None`) keeps `[system] ros_edition`: its message crates
are shared by every image, so one edition is all it can generate." The image
still gets its own edition everywhere else, so the two disagree.

Measured on a copy of `examples/linux/rust/talker` migrated with `nros dev
migrate-bringup` (2026-11-10, main at 4ff89f74):

1. `nros build native_talker_bringup:linux_jazzy` builds. The generated
   entry's manifest turns on `ros-jazzy` (`nros` and `nros-board-linux`, the
   facade's derived features), and the binary carries
   `std_msgs::msg::dds_::String_TypeHashNotSupported`, the humble placeholder.
2. `nros configure --ros-edition jazzy .` first, then the same build: the
   configure writes `RIHS01_df668c74…` into `generated/std_msgs`, and the build
   rewrites it back to `TypeHashNotSupported` before compiling. So there is no
   order of commands that builds the jazzy image correctly.

A `ros-jazzy` node with humble type hashes is issue 2260's failure (a Jazzy
peer exchanges nothing, with no error), reached from the other side: here the
declaration is right and the build drops half of it.

## Why it matters now

W6 migrates the single-node witnesses in place. The linux Rust talker is the
only witness with edition images, and it cannot move without either losing
them or building them wrong.

## Options

- **Generate per edition.** The image's messages are generated for the image's
  edition, into a tree keyed by edition (for example `generated/<edition>/`, or
  under the image's `build/<coord>/`), and the generated entry's path deps name
  that tree. It is what the leaf road did, at the cost of one codegen per
  edition a workspace uses.
- **Refuse the mismatch.** An image whose edition differs from the workspace's
  is refused at `nros build`, naming the workspace edition. It makes the defect
  loud, and edition images then need a workspace per edition, so the talker's
  six would leave the witness and `ros_editions` would build them elsewhere.

Either way, the silent build is the defect. The refusal alone is small and
could land first.

## Decision (maintainer ruling, 2026-11-11): refuse, and split

- **Refuse.** `nros configure` and `nros build` refuse an image whose ROS
  edition differs from the edition the workspace generated its messages for.
  The refusal names both editions and this issue (and 2260's failure it
  prevents); it is never a silent build.
- **Split.** The per-edition images leave the `examples/linux/rust/talker`
  witness for their own per-edition workspace(s) and fixture rows, so
  `just ros_editions build-e2e-fixtures` keeps building them.
- Per-edition message generation inside one workspace is not taken.

## Resolution (2026-11-10)

- `plan_for` refuses a workspace image whose edition is not the one the
  workspace's `generated/` was synced for (`workspace_edition_mismatch` in
  `cmd/build.rs`, unit-tested). `nros build` fails naming both editions and
  the remedy; `nros configure` reports the image as not configured and
  configures the rest. Measured on a copy of the migrated talker with an
  added `[image.linux_jazzy]`.
- `examples/linux/rust/talker` is a one-node workspace without edition
  images. `just ros_editions build-e2e-fixtures` builds its talker from a copy
  of that workspace whose `[system] ros_edition` is the edition, and copies
  the entry to `debug/talker`. Measured: the jazzy/zenoh run's talker carries
  `String_RIHS01_df668c74…`, the jazzy hash.
- The refusal text carries no issue id (the user-surface gates forbid one);
  the ids sit in a comment beside it.
