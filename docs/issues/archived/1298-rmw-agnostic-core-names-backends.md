---
id: 1298
title: "Two core crates name backends in code — `nros-macros` maps a backend name to its crate and hardcodes Cyclone's type registration; `nros-orchestration-ir` carries a module named after Cyclone"
status: resolved
type: tech-debt
area: core, rmw
severity: medium
found: 2026-09-11
resolved: 2026-10-18
related: [1219, RFC-0071, phase-444, phase-506]
---

**Home: [phase-506](../../roadmap/phase-506-api-crates-carry-only-the-api.md)** — homed 2026-10-17; listed in that phase's "Issues homed here (2026-10-17)".

## What

`check-rmw-agnostic` (issue 1219, phase-444 W4.b) baselines these under this
issue — the `packages/core/**` half of RFC-0071 § Verification, which says core
names no backend outside prose/comments:

| file | lines | shape |
| --- | ---: | --- |
| `packages/core/nros-macros/src/main_macro.rs` | 5 | `fn rmw_crate_ident`: a closed `match` `"zenoh" \| "cyclonedds" \| "xrce"` -> crate ident, which its own doc comment records as the SECOND spelling of `nros-cli-core/src/orchestration/bridge_gen.rs`'s table; and a `.filter(\|(_, rmw)\| rmw == "cyclonedds")` that emits a hardcoded `::nros_rmw_cyclonedds::register::<T>()` |
| `packages/core/nros-orchestration-ir/src/lib.rs` | 1 | `pub mod cyclonedds_type_sizing;` — a core module named after one backend, hand-mirroring that backend's `MAX_TYPES` default |

`uorb` is absent from `rmw_crate_ident`, the failure RFC-0071 used as its
motivating evidence: a closed list stops covering the tree it governs.

## Fix direction

The crate a backend lives in, and whether it needs per-type registration, are
DESCRIPTOR facts (`nros-rmw.toml`, RFC-0071 D1) — the proc-macro should receive
them from the generated selection rather than enumerate them, and the type-sizing
module should be keyed on the `needs-type-descriptors` capability. Lower the
`BASELINE` rows in `scripts/check-rmw-agnostic.py` as lines go.

## Resolved (2026-10-18) — RFC-0071 D11, phase-506 W2

* **`rmw_crate_ident` is gone.** Each backend states its registration in
  `nros-rmw.toml` (`[rmw.provides.cargo] crate` / `register`, and
  `type_register_crate` for `needs-type-descriptors`). `nros sync` writes it into
  the image's selection package as two `#[macro_export]` macros,
  `__nros_register_backends!` and `__nros_register_type!`, and `nros::main!`'s
  Zephyr and bridge arms call those. The hardcoded
  `::nros_rmw_cyclonedds::register::<T>()` went with it: a type is staged by
  whichever backend declares the capability. An entry with no selection package
  fails to BUILD naming `nros sync`, rather than linking an empty registry.
* **`cyclonedds_type_sizing` is `type_descriptor_sizing`**: it sizes the
  registry of a backend with `needs-type-descriptors`. Its knob stays
  `NROS_CYCLONEDDS_MAX_TYPES`, a backend-owned registry row (D10).

Measured on the roads the macro serves: both native bridge workspaces build and
`declarative_bridge_zenoh_to_{xrce,cyclonedds}` pass 5 of 5, including the
nested-header case that stages a type through `__nros_register_type!`; the
Zephyr `workspace-zephyr-rs-params` image builds and its `entry_e2e` cell passes.
Both `check-rmw-agnostic` rows are gone.
