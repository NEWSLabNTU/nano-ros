---
id: 225
title: "the generic type-registration seam is named cyclonedds_register — a backend name baked through the agnostic executor's call sites"
status: open
type: tech-debt
area: core
related: []
---

## Finding (deep audit 2026-07-17, C5)

`packages/core/nros-node/src/cyclonedds_register.rs` is behaviorally
agnostic (forwards through `nros_rmw::register_type_descriptor`, no direct
cyclonedds dep) but its NAME and the `cfg(rmw_cyclonedds_present)` gating
are woven through node.rs, spin.rs, and action.rs
(`crate::cyclonedds_register::MessageForRmw` bounds on generic APIs) —
axis-agnostic files carry one backend's name, and a second
descriptor-needing backend would either squat under the wrong name or fork
the seam.

## Fix sketch

Rename module + cfg to backend-neutral (`type_registry` /
`rmw_needs_type_descriptors`); mechanical, no behavior change. Keep the
cyclone-specific comment inside the module.
