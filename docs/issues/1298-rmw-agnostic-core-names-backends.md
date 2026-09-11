---
id: 1298
title: "Two core crates name backends in code — `nros-macros` maps a backend name to its crate and hardcodes Cyclone's type registration; `nros-orchestration-ir` carries a module named after Cyclone"
status: open
type: tech-debt
area: core, rmw
severity: medium
found: 2026-09-11
related: [1219, RFC-0071, phase-444]
---

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
