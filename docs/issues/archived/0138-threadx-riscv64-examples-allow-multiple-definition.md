---
id: 138
title: "qemu-riscv64-threadx Rust example CMakeLists pass -Wl,--allow-multiple-definition — conflicts with the repo-wide no-allow-multiple-def policy"
status: resolved
type: tech-debt
area: cmake
related: [phase-251, phase-277]
---

## Summary

All 6 `examples/qemu-riscv64-threadx/rust/*/CMakeLists.txt` (talker,
listener, service-server, service-client, action-client at line ~53;
action-server at line ~56) pass `"-Wl,--allow-multiple-definition"` in their
`target_link_libraries` call. This conflicts with the repo policy
established in phase-251: duplicate defined symbols must be a **link error**,
never silently masked (wrong-copy hazard). The gate
(`scripts/check-no-allow-multiple-def.sh`, allowlist
`scripts/allow-multiple-def-allowlist.txt` — intentionally empty) enforces
the invariant for the nano-ros build system, but it does **not** scan
example CMakeLists, so these 6 occurrences pass CI unnoticed.

## Why the flag is there

The riscv64 ThreadX Rust+CycloneDDS shape (phase-175.B) links a Rust
staticlib (with its own `std`/runtime objects) together with the C/C++
Cyclone backend through the same C startup path; duplicate runtime symbols
between the Rust staticlib and the platform archives are what the flag
papers over.

## Fix direction

Removal is tied to the **single-runtime** consolidation tracked from
phase-251 — once one Rust runtime copy links per image, the duplicate
definitions disappear and the flag can simply be dropped. Then:

1. Delete the flag from the 6 CMakeLists and relink
   (`just threadx_riscv64 build-fixtures`).
2. Extend `check-no-allow-multiple-def.sh` to also scan
   `examples/**/CMakeLists.txt` so the policy hole closes for good.

## Resolution (2026-07-06)

The single-runtime consolidation has already landed: the flag is now vestigial.
Removed `-Wl,--allow-multiple-definition` from all 6
`examples/qemu-riscv64-threadx/rust/*/CMakeLists.txt` and relinked — the cyclone
executables link cleanly with **no duplicate-symbol errors** (each links the app
Rust staticlib + the message C bindings + `NanoRos::NanoRos` + platform/rmw, and
only one Rust runtime copy survives). The two remaining textual references
(`nros-c/cmake/NanoRosLink.cmake`, `nros-cpp/CMakeLists.txt`) are rationale
comments noting the flag is gone — correctly ignored by the gate.

- The fixtures recipe previously built only `rust/talker` cyclone; extended it to
  build all 6 rust cyclone examples so the flag-free link is validated in CI (and
  closes a fixture-coverage gap — the other 5 had no build lane).
- Extended `scripts/check-no-allow-multiple-def.sh` to scan every in-tree
  `examples/**` + `packages/**` `CMakeLists.txt` / `*.cmake` (excluding
  build-output / generated / third-party). Gate now reports **zero uses —
  invariant fully enforced**, closing the hole that let these slip in.
