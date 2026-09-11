---
id: 1297
title: "The C/C++ API crates select and register backends BY NAME — per-backend Cargo features, `register()` calls and `extern` decls that RFC-0071 D2 says core-facing crates must not carry"
status: open
type: tech-debt
area: api, rmw
severity: medium
found: 2026-09-11
related: [1219, RFC-0071, RFC-0031, phase-444]
---

## What

`check-rmw-agnostic` (issue 1219, phase-444 W4.b) baselines these lines under
this issue. Every one is a CODE line — comments, prose strings and test code
are already excluded by the gate — naming a concrete backend inside a crate
RFC-0071 § Verification says names none:

| file | lines | shape |
| --- | ---: | --- |
| `packages/api/nros-c/Cargo.toml` | 16 | `rmw-zenoh` / `rmw-xrce` / `rmw-cyclonedds` features, their `cffi-*` aliases, `xrce-udp` / `xrce-serial`, `nros-rmw-zenoh?/…` forwarding, two optional backend deps |
| `packages/api/nros-cpp/Cargo.toml` | 8 | the twin |
| `packages/api/nros-c/src/rmw_backend.rs` | 4 | `#[cfg(feature = "rmw-zenoh")] nros_rmw_zenoh::register()` and the xrce twin |
| `packages/api/nros-cpp/src/rmw_backend.rs` | 4 | the twin |
| `packages/api/nros-c/src/lib.rs` | 1 | `#[cfg(any(feature = "rmw-zenoh", feature = "rmw-xrce"))]` |
| `packages/api/nros-cpp/src/lib.rs` | 4 | the same `cfg(any(...))`, four times |
| `packages/api/nros-cpp/include/nros/node.hpp` | 8 | `#ifdef NROS_RMW_<X>` + `extern "C" nros_rmw_<x>_register()` for all four backends |
| `packages/api/nros/Cargo.toml` | 1 | `rmw-cyclonedds = ["nros-node/needs-type-descriptors"]` |

RFC-0071's Motivation called out the Cargo half by name: *"the asymmetry IS the
special case: `rmw-cyclonedds = ["rmw-cffi"]` carries no `dep:`, because Cyclone
arrives through CMake. A user-facing library encodes one backend's build route
in its feature table."* It is unchanged, and `uorb` appears in neither manifest.
The registration half is the same closed list one layer down: a fifth backend
added out of tree (the RFC's acceptance test) needs an edit to all three files.

The umbrella row is the mildest — a capability forward with no `dep:` — but it
still spells the capability by the backend's name; D3 says the descriptor
supplies `needs-type-descriptors` through the generated selection facade.

## Fix direction

RFC-0071 D2/D3: the crates receive CAPABILITIES (`rmw-present`,
`needs-type-descriptors`) and the selection facade `nros sync` generates carries
the backend dependency and its registration. Each line removed lowers its
`BASELINE` row in `scripts/check-rmw-agnostic.py` in the same change (the gate
fails on a stale count, so it cannot regrow into the headroom).
