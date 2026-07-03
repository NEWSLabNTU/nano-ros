---
id: 138
title: "qemu-riscv64-threadx Rust example CMakeLists pass -Wl,--allow-multiple-definition — conflicts with the repo-wide no-allow-multiple-def policy"
status: open
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
