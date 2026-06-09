---
id: 5
title: Hardcoded opaque type sizes in nros-c and nros-cpp
status: resolved
type: tech-debt
area: memory
related: []
resolved_in: size_of-computed sizes
---

Opaque storage sizes for RMW handles are now computed from
`core::mem::size_of` at compile time — they always match the actual Rust
type layout and auto-adjust when types change. No manual maintenance needed.

- **nros-c**: `opaque_sizes.rs` computes sizes from `size_of::<RmwSession>()` etc.
- **nros-cpp**: `lib.rs` computes sizes from `size_of::<CppPublisher>()` etc.
