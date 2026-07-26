---
id: 289
title: "Orphaned + stale committed per-edition interface bindings (packages/interfaces/*/generated/{humble,iron}/)"
status: resolved
type: tech-debt
area: codegen
related: [issue-0269]
---

## Resolution (2026-07-26) — option 2: dropped the orphaned iron tree

Took the recommended option 2. Deleted the only orphaned per-edition tree,
`packages/interfaces/rcl-interfaces/generated/iron/` (26 files) — it was NOT a
workspace member, nothing consumed it, and its `TYPE_HASH` constants were the
`RIHS01_0000…0` placeholder (the latent trap this issue flagged).

Kept the `generated/humble/` trees: contrary to the original "fully orphaned"
read, they ARE live — the root `Cargo.toml` lists them as workspace members
("Required by nros-node's param-services feature"), so a real dependency
consumes them. The per-edition *directory* layout there is just how those
committed bindings are stored, not a per-edition selection mechanism.

`cargo metadata` still resolves after the deletion. A workspace targeting one
edition regenerates in place (the standard-ROS single-edition model); shipping
prebuilt multi-edition bindings (option 1) is not pursued. Rolling was dropped
as a supported edition in the same pass (removed `RosEdition::Rolling`).

## Summary

`packages/interfaces/*/generated/{humble,iron}/` are committed per-edition
generated-binding trees from an early "ROS edition-aware binding" attempt
(commit `23e12afec`). They are **orphaned and partly stale**:

- **Orphaned** — nothing consumes them by edition. The parent crates (e.g.
  `packages/interfaces/rcl-interfaces/Cargo.toml`) carry NO path dep on the
  `generated/<edition>/nros-*` crates, and the bundled-interface resolver
  (`cmake/compat/stubs/_NrosFindRosMsgPackage.cmake` layer 3) returns the parent
  `packages/interfaces/<pkg>` dir, never a per-edition subdir. No build selects
  `generated/<edition>/`.
- **Stale** — coverage is uneven (only `rcl-interfaces` has an `iron/` tree;
  others are humble-only), and the `iron/` copy predates phase-304 W1: its
  `TYPE_HASH` constants are the `RIHS01_0000…0` placeholder digest, NOT the real
  REP-2011 hash the `rosidl_codegen::rihs` engine now computes.

## Why it matters

Dead code that *looks* authoritative — a committed `iron` binding with a wrong
(placeholder) type hash is a latent trap if anything ever wires it up. Surfaced
while assessing phase-304 W2b leg (c) (`generated/<edition>/` interface dir),
which was found unnecessary for the workspace codegen path: a workspace targets
ONE edition and regenerates in place (content-based `write_if_changed` + the
CMake args-file `ros_edition` + codegen `add_custom_command DEPENDS` re-trigger).
This committed-vendored multi-edition layout is a *separate* idea from that
lowering.

## Options

1. **Regenerate + wire** — if nano-ros wants to SHIP prebuilt multi-edition
   bindings (so users need no ROS install), regenerate every
   `generated/<edition>/` with the W1c engine (real hashes) AND make the parent
   crate + resolver select by `[system].ros_edition`. This is the RFC-0056 §
   "per-edition interop profile" vision made real.
2. **Drop to humble-only + regenerate-on-demand** (recommended default) — delete
   the orphaned `iron/` trees; keep committed bindings humble-only (or drop
   committed generated entirely) and rely on `nros ws sync` / the CMake codegen
   path to generate the declared edition's bindings on demand. Matches standard
   ROS (single-edition per workspace).

Cross-ref: phase-304 (`docs/roadmap/phase-304-complete-ros-edition-axis.md`,
W2b note), RFC-0056 (`docs/design/0056-ros-edition-axis.md`).
