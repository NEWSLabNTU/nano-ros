---
id: 62
title: D3 completion — one registration path + generated link-manifest + weak-default deletion (rides single-runtime)
status: open
type: tech-debt
area: build
related: [issue-0042, issue-0050, phase-241, phase-247]
---

## Progress (2026-06-14)

- **R1 — DONE.** Dispatch is data on `RmwDispatch` (`resolve_rmw()`), rendered to
  `cmake/NanoRosRmwDispatch.cmake` (`nros_rmw_dispatch`), drift-guarded by
  `rmw_cmake_dispatch_is_current`. The W11 synth (`NanoRosRuntimeCrate.cmake`) pulls its
  cffi feature from it; the hardcoded backend→feature map is gone. The platform-specific
  cyclonedds *link wiring* stays in cmake (keys off the manifest's `NROS_RMW_NEEDS_CXX_LINKER`
  / `EXTRA_LINK_LIBS` when `NanoRosLink.cmake` is reworked under R2/R3).
- **R2 — BLOCKED on R3, NOT a plain deletion.** Audit: the weak default and the cmake
  stub are BOTH load-bearing — hosted needs the weak no-op to satisfy `nros_support_init`'s
  *unconditional* call (the `.init_array` ctor does the real registration); bare-metal
  startup does NOT walk `.init_array`, so the cmake strong stub is the *only* registration
  path there. Deleting either breaks a path. R2 must follow the R3 one-trigger restructure
  (a single guaranteed registration) before the weak default + stub can die. Also: preserve
  the `nros_platform_log_{write,flush}` weak fallbacks living in the same TU.

## Why

RFC-0042 §D3 has four goals. The **single shared runtime** model
([phase-241-d3-single-runtime](../roadmap/phase-241-d3-single-runtime.md), Stable
+ in progress) delivers the hardest one — *no papering-over* (bullet 3): one Rust
staticlib per binary ⇒ `std`/`compiler-builtins` monomorphized once ⇒
`--allow-multiple-definition` removable for real. But two D3 goals + one
[issue 0050](0050-weak-symbol-audit-and-checkers.md) item are **not** closed by
single-runtime, and the phase doc says so itself: *"the single registration path
of D3's first bullet is satisfied by that stub; the linkme distributed-slice
remains the pure-Rust-binary path."* So:

- **Bullet 1 (one registration path) — NOT done.** Up to three triggers still
  coexist: the cmake-generated `nros_app_register_backends` stub (C/C++ path), the
  re-enabled `linkme` slice (pure-Rust path), and the W11 synthesized-runtime
  `.init_array` ctor anchor. That is the same two-contract split D3 set out to
  collapse — made non-fragile by single-runtime, but not unified.
- **Bullet 2 (generated link manifest) — NOT done.** The "RMW backend dispatch"
  is hand-maintained prose (`zenoh/xrce → rlib`; `cyclonedds → +libnros_rmw_
  cyclonedds +libddsc +libstdc++ even for C`). That per-backend link requirement
  is exactly a manifest entry, today re-derived in cmake conditionals.
- **issue 0050 W3.1 (delete the weak `nros_app_register_backends`) — open.** The
  weak no-op default + the cmake strong stub are the #48-class hazard; single-
  runtime keeps both.

This issue tracks finishing bullets 1+2 and closing 0050 W3.1, **on top of** the
single-runtime foundation — not a competing architecture. (Origin: a parallel
A→C exploration whose "generated register table" framing was wrong for the
synthesized-crate world, but whose underlying ideas — `resolve_rmw` as the one
SSoT, the dispatch table as generated data, the ctor as the single guaranteed
registration that lets the weak default die — apply directly.)

## Work items

- **R1 — dispatch table → generated data (finishes bullet 2).** Emit
  `backend → {rlib dep, extra link libs}` as data from `resolve_rmw()`
  (`packages/cli/cargo-nano-ros/src/rmw_resolver.rs`, the existing RFC-0031
  lowering SSoT), consumed by **both** the W11 synthesized `nros_ws_runtime/
  Cargo.toml` backend feature **and** the cmake link extras (the Cyclone
  `libstdc++`/`libddsc` wiring). Removes the drift between "which feature the
  synthesized crate sets" and "which libs cmake adds"; turns the locked
  Cyclone-libstdc++ choice into one generated entry.

- **R2 — close 0050 W3.1 through the single-runtime ctor.** Once the W11
  synthesized-runtime `.init_array` ctor anchor (`nros_cpp_auto_register_backend`)
  *guarantees* backend registration on the umbrella path, the cmake-generated
  `nros_app_register_backends` stub **and** the weak default in
  `nros-c`/`nros-cpp` `c-stubs/weak_register_backends.c` become redundant →
  delete them → the #48 weak-no-op hazard is gone. The phase-247 weak-symbol
  **image gate** (`scripts/check-weak-symbols-image.sh`) already asserts the
  registration symbol resolves strong, so it guards the deletion.

- **R3 — fold the triggers into one (finishes bullet 1).** **Designed →
  [phase-249](../roadmap/phase-249-one-registration-trigger.md).** Audit (2026-06-14)
  corrected the original framing: the ctor anchor is NOT the natural single trigger —
  `.init_array` isn't walked on all bare-metal, and linkme is RTOS-blind. The only
  universal mechanism is the **explicit generated `nros_rmw_<backend>_register()` call**
  (C/C++: generated strong `nros_app_register_backends`; Rust: `main!`/board-entry from
  the R1 manifest). Phased P1–P4 (migrate before delete), per-platform e2e gated; **P4 is
  R2** (deletes the weak default + stub). See phase-249 for the full plan.

## Acceptance

- The RMW backend's rlib dep + extra link libs are emitted once from
  `resolve_rmw` and consumed by both the synthesized runtime crate and cmake
  (no hand-maintained dispatch prose) — R1.
- The weak `nros_app_register_backends` default + the cmake stub are deleted; a
  missing registration is a link error, not a silent no-op; the weak-symbol gates
  green — R2, closes [issue 0050](0050-weak-symbol-audit-and-checkers.md) W3.1.
- (Stretch) one registration trigger across C/C++ + pure-Rust + embedded — R3.

## References

- [phase-241-d3-single-runtime](../roadmap/phase-241-d3-single-runtime.md) — the
  foundation this rides on (W1–W12 + the W11 Option D synthesized runtime).
- [RFC-0042](../design/0042-platform-build-determinism.md) §D3 — the four goals;
  bullet 3 done, 1+2 here.
- [issue 0050](0050-weak-symbol-audit-and-checkers.md) W3.1 — the weak-default
  deletion R2 closes.
- [phase-247](../roadmap/phase-247-weak-symbol-determinism.md) — the weak-symbol
  gates that guard R2.
