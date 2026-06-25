---
id: 100
title: "Baremetal standalone examples split into a sibling node pkg — break the copy-out self-containment contract"
status: open
type: bug
area: testing
related: [phase-244, rfc-0026]
---

## Summary

The `examples/qemu-arm-baremetal/rust/*` and `examples/stm32f4/rust/*` standalone examples
are structured as a **two-crate workspace split** — an Entry binary plus a sibling node
package — instead of a single self-contained crate. The Entry path-deps and `[patch]`es
**up into a sibling example directory**, which breaks the standalone copy-out contract
(CLAUDE.md: "Examples are standalone copy-out projects … no workspace walk-up").

Concretely, for `examples/qemu-arm-baremetal/rust/talker`:

```
talker/src/main.rs   nros::main!();                 # Form-1 self-bringup Entry
talker/src/lib.rs    pub use talker_pkg::register;  # re-export only — no logic
talker/Cargo.toml    talker_pkg = { path = "../talker_pkg" }
                     [patch.crates-io]
                       std_msgs          = { path = "../talker_pkg/generated/std_msgs" }
                       builtin_interfaces = { path = "../talker_pkg/generated/builtin_interfaces" }
talker_pkg/          # the actual node + package.xml + generated/   ← SIBLING dir
```

Copying just `talker/` out leaves `../talker_pkg` and `../talker_pkg/generated/` dangling —
the project does not build standalone. (Walk-up to `../../../../packages/…` framework crates
is fine; those are rewritten on copy-out. Walk-up to a **sibling example dir** is the break.)

The split landed in Phase 244.D1 to make node logic "RMW/platform-agnostic" (a workspace-style
reuse pattern) — but the Entry-pkg + node-pkg shape is a **workspace** concept; a standalone
`examples/<plat>/<lang>/<example>/` must be self-contained. `examples/native/rust/talker` is
the correct reference: its own `package.xml` + `generated/` + `src`, no sibling pkg.

## Symptom that surfaced it

`just check` → `check-dep-chain` + `native::check` fail on a pristine checkout (no codegen run
yet):

```
cargo tree (examples/qemu-arm-baremetal/rust/talker)
  → failed to read .../talker_pkg/generated/builtin_interfaces/Cargo.toml (No such file)
native check: 12 example(s) are missing their generated message bindings
  examples/qemu-arm-baremetal/rust/{listener_pkg, serial_talker_pkg, xrce_talker_pkg, …}
```

The dep-chain check's step-2 heuristic (`if [ -f "$ex/package.xml" ]` → codegen, else skip) is
**correct**: an Entry declares no interfaces, so it rightly has no `package.xml`. The bug is
that the *example* puts `package.xml` (and `generated/`) in a sibling the Entry walks up to,
which the check never visits. Workaround for now: `just generate-bindings` (walks every pkg)
populates the sibling `generated/`, then `just check` is green. But the example is still not
copy-out-able.

## Fix direction

Collapse each split standalone into one self-contained crate (mirror `examples/native/rust/talker`):
- `talker/package.xml` (declares `std_msgs`) — the Entry dir becomes the ROS package.
- `talker/src/lib.rs` = the node logic moved from `talker_pkg/src/lib.rs` (not a re-export);
  `nros::main!()` Form-1 self-bringup dispatches to the same-crate `register`.
- `talker/generated/` = its own codegen; `[patch]` → `generated/std_msgs` (own dir).
- Drop the `talker_pkg = { path = "../talker_pkg" }` dep and remove the `talker_pkg/` dir.

After this the dep-chain check needs no change — `$ex/package.xml` exists, codegen runs
in-place, no sibling-walk, no special-casing.

## Scope

- `examples/qemu-arm-baremetal/rust/` — 15 `*_pkg` sibling dirs.
- `examples/stm32f4/rust/` — 7 `*_pkg` sibling dirs.
- `examples/esp32`, `examples/qemu-esp32-baremetal` — 0 (already single-crate; spot-check).
- Reference (correct shape): `examples/native/rust/talker` — own `package.xml` + `generated/`,
  no sibling.

Mechanical but broad; land per-example so each `just <plat>` / copy-out is verified.

## Evidence

Found 2026-06-25 chasing the `just check` dep-chain + `native::check` failures while closing
out issue #98 (a separate node-naming fix). The maintainer confirmed the intended contract:
the Entry-pkg shape exists only for workspaces; `examples/<plat>/<lang>/<example>` is a
standalone app and must be self-contained.
