---
id: 92
title: nros-node `lifecycle-services` feature build broken — unresolved `EmbeddedServiceServer`
status: open
type: bug
area: core
related: [phase-264]
---

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

## Refined finding (2026-06-20)

The type **still exists** — `pub struct EmbeddedServiceServer<…>` at
`executor/handles.rs:1797` (not cfg-gated), re-exported via `pub use handles::*`
(`executor/mod.rs:77`). Yet `use crate::executor::EmbeddedServiceServer` does not
resolve under `--features lifecycle-services`. So this is **feature-gating**, not a
rename: `lifecycle-services` (`= ["dep:nros-lifecycle-msgs", "alloc"]`) likely misses a
service-infra feature that makes the service-server path / its `pub use` visible (note
`executor/mod.rs:34` `#[cfg(feature = "std")]` near the `handles` items).

## Fix

Add the missing service-infra feature edge to `nros-node`'s `lifecycle-services` (the
services the lifecycle path needs), then re-run the W2 verification (a `[lifecycle]
autostart` workspace built via `nros::main!` registers the 5 services).

> **Number note:** filed as 0090 → collided with the resolved/archived ThreadX-C
> 0090; renumbered to 0092.
