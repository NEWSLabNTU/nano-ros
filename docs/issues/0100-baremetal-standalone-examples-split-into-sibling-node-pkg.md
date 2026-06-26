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
- **W5** `1956c869b` — `qemu-arm-baremetal/rust/talker-rtic` (RTIC pilot; validates the
  `node_pkgs` self-reference recipe below).
- **W5 fan-out** `6124de25f` — the 7 remaining baremetal RTIC examples (talker-rtic-mixed,
  listener-rtic, listener-rtic-mixed, action-{client,server}-rtic, service-{client,server}-rtic).

**qemu-arm-baremetal user examples: DONE** (6 declarative + 8 RTIC = 14, all build-verified).
- **W6a** `f543ea375` — stm32f4 `listener-rtic`, `listener-embassy` (clean single-node).
- **W6b** `57f1ff4f5` — stm32f4 `talker-rtic` + `talker-embassy` (SHARED `stm32f4_talker_pkg`
  duplicated into both entries).

stm32f4 build notes: rtic entries `cargo build` (thumbv7em); **embassy** entries only
`cargo check` — their full-link failure (missing platform C symbols) is PRE-EXISTING (the
embassy stm32f4 board `init_hardware` is a `todo!()` stub; reproduced on un-collapsed HEAD),
unchanged by the collapse. The talker/listener nodes use a local `PlaceholderInt32` (no
std_msgs), so their managed set is just nros-core/nros-serdes.

**Remaining: stm32f4 action + service pairs (W6c/d) — the cross-pkg case.**
`action-client-rtic`/`service-client-rtic` `use stm32f4_{action,service}_server_pkg::Placeholder*`
(a hand-written `PlaceholderAct`/`PlaceholderSrv` `RosAction`/`RosService` + `PlaceholderInt32`
defined in the SERVER pkg, ~lines 146-196 of `action_server_pkg/src/lib.rs`). Recipe per pair:
collapse the server normally (placeholder stays in `*-server-rtic/src/lib.rs`); collapse the
client by moving its node lib in AND **inlining a copy of the placeholder block** (+ its
`use` imports), replacing the `use stm32f4_*_server_pkg::Placeholder*;` line — duplication is
correct for standalone copy-out. Delete both `*_pkg` dirs after.

Plus: baremetal e2e-fixture `*_pkg` (phase216-rtic-e2e, qemu-baremetal-main-e2e — test infra,
confirm in-scope).

Done: 6 declarative examples (all build-verified — thumbv7m / thumbv7em). **Remaining splits
are NOT clean 1:1 declarative folds** and need a dedicated per-shape wave:

- **RTIC / Embassy** (qemu-arm-baremetal `*-rtic` + `*-rtic-mixed`; stm32f4 `*-rtic` +
  `*-embassy`) — **design validated, recipe below** (pilot `talker-rtic`, `1956c869b`).
- **Embassy** (stm32f4 `talker-embassy`, `listener-embassy`) — Embassy executor shape.
- **Shared node pkgs** — stm32f4 `talker_pkg` is path-dep'd by BOTH `talker-rtic` and
  `talker-embassy`; `listener_pkg` by multiple. A 1:1 fold is impossible; collapse must
  DUPLICATE the node logic into each Entry (acceptable for standalone copy-out examples).
- **Cross-pkg deps** — stm32f4 `action_client_pkg → action_server_pkg`,
  `service_client_pkg → service_server_pkg` (a node pkg path-deps another node pkg).
- **e2e fixtures** — `phase216-rtic-e2e`, `qemu-baremetal-main-e2e` (`*_pkg`): test infra,
  confirm in-scope before touching.

### RTIC / Embassy collapse recipe (validated, pilot `1956c869b`)

RTIC/Embassy Entry pkgs have **no `src/lib.rs` re-export**. `nros::main!()` finds the node set
from `[package.metadata.nros.entry] node_pkgs = ["<pkg>"]` and emits
`::<pkg_ident>::register_dispatch(&executor)` per entry (`nros-macros/src/main_macro.rs`
~902-950; `pkg_to_crate_ident` maps `-`→`_`). The macro accepts a `node_pkgs` entry that
**self-references the Entry crate**, so each split collapses into one bin+lib crate. (The node
logic is already the identical declarative `Node` + `ExecutableNode` + `nros::node!` — no RTIC
code lives in the node; `nros::node!` emits the `register_dispatch` the RTIC path calls.)

Per example (`<entry>` ⇐ `<pkg>`):
1. `git mv <pkg>/package.xml <entry>/package.xml`; set `<name>` to the entry crate (underscored).
2. `cp <pkg>/src/lib.rs <entry>/src/lib.rs` (NEW — the entry had none).
3. `<entry>/Cargo.toml`: add `[lib] crate-type = ["rlib"]`; change
   `node_pkgs = ["<pkg>"]` → `["<entry-cargo-name>"]` (self); add `[package.metadata.nros.node]`
   (class `<entry_underscored>::<NodeStruct>`, copy name/default_namespace/dispatch from `<pkg>`);
   drop the `<pkg>` path-dep; add the node's `nros-log` + `std_msgs` deps; remove the hand `[patch]`.
4. `rm` `<pkg>/`; `nros sync <entry>` writes the managed patch → own `generated/`. Build-verify.

Note: every RTIC/Embassy entry is **single-node** (`node_pkgs = ["<one>"]`) and has no `_entry`
suffix, so the self-reference is unambiguous. For the stm32f4 **shared** `stm32f4_talker_pkg`
(talker-rtic + talker-embassy) the lib.rs is copied into BOTH entries (duplication is correct
for standalone copy-out). For the **cross-pkg** action/service client examples, the shared
`PlaceholderAct`/placeholder type in `*_server_pkg` must be inlined into (or duplicated across)
the client + server entries — they currently `use stm32f4_action_server_pkg::PlaceholderAct`.

Final step (after every baremetal example is self-contained): drop the now-redundant two-pass
codegen loop in `just/qemu-baremetal.just` (the split was its only reason to exist).

## Evidence

Found 2026-06-25 chasing the `just check` dep-chain + `native::check` failures while closing
out issue #98 (a separate node-naming fix). The maintainer confirmed the intended contract:
the Entry-pkg shape exists only for workspaces; `examples/<plat>/<lang>/<example>` is a
standalone app and must be self-contained.
