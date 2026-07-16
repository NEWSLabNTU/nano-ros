---
id: 211
title: "Kconfig→rustc-env locator-bake build.rs copy-pasted into every zephyr rust example and workspace zephyr_entry"
status: resolved
type: tech-debt
area: examples
related: [phase-291]
---

## Problem (audit 2026-07-16, J1/I1)

`examples/zephyr/rust/*/build.rs` and every workspace `zephyr_entry`
`build.rs` (`ws-lifecycle`/`ws-params`/`ws-qos`/`ws-realtime`/`ws-safety`
rust variants, `workspaces/rust`) carry a near-identical file: the
Kconfig→cfg bridge plus the CONFIG_NROS_ZENOH_LOCATOR / DOMAIN_ID / XRCE
agent rustc-env bake (the documented known-issue #17 empty-locator
workaround). Copy-out users inherit ~50 lines of build plumbing, and a fix
to the workaround means N edits.

## Fix sketch

A small `nros-zephyr-build` helper crate (build-dependency) owning kconfig
export + locator baking; each example's build.rs collapses to one call.


## Resolution (2026-07-16, phase-291)

`packages/core/nros-zephyr-build` now owns the canonical bake
(`bake_nros_config()`: locator + domain + the issue-0163 XRCE synthesis,
zero-dep so it stays host-checkable — upstream `zephyr-build` is
west-module-path-only and its `export_kconfig_bool_options()` call stays in
the leaf). All **14** leaves collapsed to the 4-line `build.rs` — the issue's
inventory said 13; the phase-291 W4 grep-gate immediately found the 14th
(`examples/zephyr/rust/cyclonedds/talker-aemv8r`, which had NO bake at all —
the issue-0161 silent-domain-0 latent class; its FVP-rust build lane is red
at baseline — pre-existing toolchain rot, filed as #216). The 7 workspace entries gained
the XRCE block they had drifted away from. `nros sync` resolves the helper
via one `nros_crate_path_lookup()` entry (leaf-local patch).

Guard: `example_shape::zephyr_leaf_buildrs_uses_shared_bake` — no
`bake_kconfig_*` / local `fn kconfig_line` copies under `examples/`, every
zephyr rust leaf calls the shared bake, ≥14 leaves walked (silent-empty
guard). Proof: full zephyr west fixture rebuild + 67-test `binary(~zephyr)`
sweep green (phase-291 W3).
