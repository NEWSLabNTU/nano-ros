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
- Reference (correct shape): `examples/qemu-esp32-baremetal/rust/talker` — single-crate
  declarative (`[lib]` node + `[[bin]]` entry, both `.entry`+`.node` metadata, own
  `package.xml` + `generated/`, patch in `.cargo/config.toml`). (`native/rust/talker` is the
  *imperative* shape — not a mirror for the declarative `nros::main!()` embedded examples.)

Mechanical but broad; land per-example so each `just <plat>` / copy-out is verified.

## Progress (waves)

Per-example collapse recipe (declarative `nros::main!()` shape): move `<pkg>/package.xml`
→ `<entry>/package.xml` (rename `<name>` to the entry crate); move `<pkg>/src/lib.rs` node
logic → `<entry>/src/lib.rs` (replaces the `pub use <pkg>::register` re-export); in
`<entry>/Cargo.toml` drop the `<pkg>` path-dep, add the node's `nros-log` + `std_msgs` deps +
`[package.metadata.nros.node]` (class re-pathed to the entry crate), remove the hand
`[patch.crates-io]`; `rm` the `<pkg>/` sibling; `nros sync` writes the managed patch into
`<entry>/.cargo/config.toml` → own `generated/`. Build-verify thumbv7m.

- **W1** `8cf597523` — `qemu-arm-baremetal/rust/talker` (pilot).
- **W2** `563350f0d` — `qemu-arm-baremetal/rust/listener`.
- **W3** `fb3b7b15b` — `qemu-arm-baremetal/rust/{serial-talker,serial-listener,talker-xrce}`.
- **W4** `c6284c3a1` — `stm32f4/rust/talker` (the one clean 1:1 in stm32f4).

Done: 6 declarative examples (all build-verified — thumbv7m / thumbv7em). **Remaining splits
are NOT clean 1:1 declarative folds** and need a dedicated per-shape wave:

- **RTIC** (qemu-arm-baremetal `*-rtic` + `*-rtic-mixed` ≈9; stm32f4 `*-rtic` ≈5) — the
  `#[rtic::app]` Entry integrates the node differently than `nros::main!()`; study one first.
- **Embassy** (stm32f4 `talker-embassy`, `listener-embassy`) — Embassy executor shape.
- **Shared node pkgs** — stm32f4 `talker_pkg` is path-dep'd by BOTH `talker-rtic` and
  `talker-embassy`; `listener_pkg` by multiple. A 1:1 fold is impossible; collapse must
  DUPLICATE the node logic into each Entry (acceptable for standalone copy-out examples).
- **Cross-pkg deps** — stm32f4 `action_client_pkg → action_server_pkg`,
  `service_client_pkg → service_server_pkg` (a node pkg path-deps another node pkg).
- **e2e fixtures** — `phase216-rtic-e2e`, `qemu-baremetal-main-e2e` (`*_pkg`): test infra,
  confirm in-scope before touching.

Final step (after every baremetal example is self-contained): drop the now-redundant two-pass
codegen loop in `just/qemu-baremetal.just` (the split was its only reason to exist).

## Evidence

Found 2026-06-25 chasing the `just check` dep-chain + `native::check` failures while closing
out issue #98 (a separate node-naming fix). The maintainer confirmed the intended contract:
the Entry-pkg shape exists only for workspaces; `examples/<plat>/<lang>/<example>` is a
standalone app and must be self-contained.
