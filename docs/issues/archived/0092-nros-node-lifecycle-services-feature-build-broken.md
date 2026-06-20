---
id: 92
title: nros-node `lifecycle-services` needs an RMW backend (bare feature build fails)
status: resolved
type: enhancement
area: core
related: [phase-264]
resolved_in: "gate lifecycle_services + parameter_services modules on has_rmw (e129cb0da)"
---

## Resolved (2026-06-20)

Encoded the "needs an RMW backend" requirement in the cfg: the `lifecycle_services`
and `parameter_services` modules (which build `executor::EmbeddedServiceServer`,
itself `#[cfg(any(has_rmw, test))]`) are now gated
`#[cfg(all(feature = "…-services", any(has_rmw, test)))]`. A bare
`cargo build -p nros-node --features lifecycle-services` (no RMW) compiles clean
(the module is correctly absent — service servers are meaningless without a
backend); with an RMW (every shipping app/entry) the module is present + compiles;
the `test` arm keeps it for test builds. Verified all three.

## Problem

`cargo build -p nros-node --features lifecycle-services` fails:

```
error[E0432]: unresolved import `crate::executor::EmbeddedServiceServer`
   --> packages/core/nros-node/src/lifecycle_services.rs:325
325 | use crate::executor::{EmbeddedServiceServer, NodeError};
    |                       ^^^^^^^^^^^^^^^^^^^^^ no `EmbeddedServiceServer` in `executor`
```

`crate::executor` no longer exports `EmbeddedServiceServer` (renamed / moved /
feature-gated away), but `lifecycle_services.rs` still imports it. So the
`lifecycle-services` feature does not compile.

**Pre-existing** — reproduces with a clean tree (verified 2026-06-20 by stashing the
phase-264 W2 changes; the error is unchanged). Independent of W2.

## Impact

Blocks verifying phase-264 W2 (`nros::main!` lifecycle wiring) with the feature on,
and any cargo consumer that enables `nros/lifecycle-services` (the declarative
lifecycle path). The bake path likely hits it too once a `[lifecycle]` system is built.

## Root cause + downgrade (2026-06-20)

NOT a hard breakage — a **backend-less** build only. `mod handles` + `pub use
handles::*` (`executor/mod.rs:33,77`) are `#[cfg(any(has_rmw, test))]`, so
`EmbeddedServiceServer` exists only when an RMW backend is present (`has_rmw`, set by
`nros-node/build.rs` from `CARGO_FEATURE_RMW_*`). `lifecycle_services.rs` (gated on the
`lifecycle-services` feature) imports it unconditionally, so bare `nros-node --features
lifecycle-services` (no backend) fails. **Real consumers are fine:** any entry deps a
board → an RMW backend → `has_rmw`; verified `nros --features
"lifecycle-services,rmw-cffi"` builds clean, and `ws-lifecycle-rust` (board + zenoh)
links. So phase-264 W2 is **verified**; this is a minor robustness gap.

## Fix (low priority)

Gate `mod lifecycle_services` (and its import) on `#[cfg(all(feature =
"lifecycle-services", any(has_rmw, test)))]`, OR make `lifecycle-services` imply an RMW
edge — so the bare feature build fails loud (or is a no-op) instead of an unresolved
import. Cosmetic; does not block any real build.

> **Number note:** filed as 0090 → collided with the resolved/archived ThreadX-C
> 0090; renumbered to 0092.
