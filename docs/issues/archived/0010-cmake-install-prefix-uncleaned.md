---
id: 10
title: CMake install prefix is never cleaned between builds
status: resolved
type: bug
area: cmake
related: [phase-140]
resolved_in: Phase 140
---

Resolved by Phase 140 (install-local rip-off). Pre-Phase-140,
`just install-local` ran `cmake --install` for each RMW backend into a
shared `build/install/` prefix; CMake install is additive, so stale
`libnros_cpp_ffi_zenoh.a` / `libnros_cpp_ffi_xrce.a` archives accumulated
across builds. Phase 140 deleted `install-local` entirely — per-example
builds produce their own Corrosion target tree and never reuse a shared
prefix, so the stale-artefact failure mode is gone. Verified: `install-local`
removed and each example uses per-example Corrosion.
