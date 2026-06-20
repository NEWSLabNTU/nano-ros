---
id: 90
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

## Fix

Point the import at the current location of the embedded service-server type (or its
replacement), or restore the export. Then re-run the W2 lifecycle verification (a
`[lifecycle] autostart` workspace built via `nros::main!` registers the 5 services).
