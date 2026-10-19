---
id: 1297
title: "The C/C++ API crates select and register backends BY NAME — per-backend Cargo features, `register()` calls and `extern` decls that RFC-0071 D2 says core-facing crates must not carry"
status: resolved
type: tech-debt
area: api, rmw
severity: medium
found: 2026-09-11
resolved: 2026-10-19
related: [1219, RFC-0071, RFC-0031, phase-444, phase-506]
---

**Home: [phase-506](../../roadmap/archived/phase-506-api-crates-carry-only-the-api.md)** — homed 2026-10-17; listed in that phase's "Issues homed here (2026-10-17)".

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

## 2026-10-18 — `node.hpp`'s register chain is deleted (RFC-0071 D9)

The `#ifdef NROS_RMW_<X>` + `extern "C" nros_rmw_<x>_register()` lines had no
caller in the headers: registration is the generated strong
`nros_app_register_backends()`, and the two hand callers (the PX4 module
template and `nros-px4-register-check`) declare the symbol themselves. Its
`check-rmw-agnostic` baseline row (8 lines) is gone. The rest of this issue is
phase-506 W4.

## 2026-10-18 — the cmake road no longer uses the features (phase-506 W4a)

Every C/C++ configure that bundles an rlib backend imports a generated image
crate (`cmake/NanoRosImageCrate.cmake`) instead of `nros-c` / `nros-cpp` with an
`rmw-<name>` feature: the image depends on the API crate without a backend and
on the backend crate the descriptor names (`[rmw.provides.cargo]
register_crate` / `register_dir`), forwarding the platform through the
descriptor's new `[rmw.provides.cargo.platform_features]` table, and anchors its
`register`. The features themselves stay until the Zephyr west road, the NuttX
Makefile and PX4 move too (W4b/W4c); s32ds keeps a shim.

## Resolution (2026-10-19) — phase-506 W4d

The features are gone. `nros-c` and `nros-cpp` keep `rmw-cffi` and lose
`rmw-zenoh` / `rmw-xrce` / `rmw-cyclonedds`, the `cffi-*` aliases,
`rmw-<name>-cffi`, `xrce-udp` / `xrce-serial`, every `nros-rmw-zenoh?/…`
forward and both optional backend deps. `nros-c/src/rmw_backend.rs` is deleted;
`nros-cpp`'s shrank to `metadata_backend.rs`, the recording backend's anchor
under `metadata-mode`, which is not a backend an image selects. `nros`'s
`rmw-cyclonedds` marker is renamed for its capability,
`needs-type-descriptors`, and the facade turns it on for any linked backend
whose descriptor names a `type_register_crate`.

Every road that bundles an rlib backend builds the generated image crate: the
cmake road (W4a), Zephyr west (W4b), the NuttX Makefile (W4c), and now PX4's
bridge recipe and the link-determinism fixture through the generator's `-P`
script mode. The descriptor fields that named the old features are refused if
stated (`[rmw.link] c_cffi_feature`, `[rmw] cffi_feature`), the `rmw-<x>-cffi`
`package.xml` announcements are dropped, `nros-c` no longer publishes
`cargo:rmw_backends` (each backend announces itself over its own `links` key),
and `check-nros-c-feature-agreement` now refuses any glue that still asks an
API crate for a backend.

`check-rmw-agnostic`'s baseline is EMPTY: 41 lines over 7 files to 0.
