---
id: 15
title: threadx-linux C++ nros_cpp_ffi.h regeneration race
status: resolved
type: bug
area: threadx
related: [phase-226]
resolved_in: cross-process flock
---

Surfaced by Phase 226.E/226.F: intermittent `nros_cpp_ffi.h` "multiple
definition / conflicting declaration of `nros_cpp_qos_t`" during cold
threadx-linux C++ fixture builds. The committed cbindgen headers
(`nros_cpp_ffi.h`, nros-c's `nros_generated.h`) are regenerated in place by
every parallel `build.rs`; threadx-linux C++ fixtures run unserialized, so N
Corrosion/Cargo trees raced the write/compare/rename with no cross-process
mutual exclusion. The pre-existing atomic tmp+rename only guards a single
writer.

Fixed in `packages/core/nros-build-helpers/src/shared.rs`: header
regeneration acquires a cross-process advisory `flock` keyed on the absolute
output path (temp-dir lockfile; non-unix falls back to atomic-rename-only)
around the atomic write, making "generate → atomically replace" mutually
exclusive across all concurrent regenerators. Covered by
`header_lock_serializes_concurrent_holders` (8×200 concurrent acquisitions,
zero overlap). Also protects nros-c's `nros_generated.h`.
