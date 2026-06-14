---
id: 61
title: zephyr/CMakeLists.txt passes removed nros-c/nros-cpp features (phase-248 C3.2 downstream)
status: wontfix
type: bug
area: zephyr
related: [phase-248, issue-0060, issue-0058, issue-0059, phase-241]
---

## Resolution (2026-06-14) — premise void, C3.2 superseded by 241.D3

This issue assumed phase-248 **C3.2** (`d44a555c1`) had REMOVED the
`platform-*` / `cffi-{zenoh-cffi,xrce-c}` / `rmw-{zenoh,xrce}` cargo features
from `nros-c`/`nros-cpp`. That commit was **dropped during the rebase onto a
main that had meanwhile landed Phase 241.D3-rev** (single-runtime umbrella):
D3 deliberately RE-COUPLES `nros-c`/`nros-cpp` to one board-selected backend
rlib (`rmw-zenoh = ["rmw-cffi", "dep:nros-rmw-zenoh"]` + `src/rmw_backend.rs`
force-link) to eliminate the multi-staticlib double-cffi-instance hazard
(`libnros_c.a` + `libnros_rmw_zenoh.a` each bundling a crate-hash-distinct
zenoh-pico). The C/C++ staticlib ROOT is the sanctioned place to bundle the
backend; the `platform-*`/`rmw-*` features on `nros-c`/`nros-cpp` are the
**board-driven selectors** for that bundled backend, not user leakage — so
they REMAIN on `main`. `zephyr/CMakeLists.txt` therefore still passes features
that still exist → no cargo-feature-resolution break. Closing `wontfix`.
(The platform-agnostic `src/lib.rs` — no `#[cfg(feature="platform-*")]` — that
C3.2's Phase-1 delivered is independently present on `main` via D3's
`global-allocator`/`critical-section` vtable routing, so that half stands.)

## The break (original report — no longer applies, see Resolution above)

Phase-248 C3.2 (`d44a555c1`) made `nros-c`/`nros-cpp` RMW/platform-agnostic —
removed their `platform-zephyr`, `cffi-{zenoh-cffi,xrce-c}`,
`rmw-{zenoh-cffi,xrce-cffi}` cargo features. `zephyr/CMakeLists.txt` still passes
those (now-nonexistent) features to the nros-c/nros-cpp corrosion builds
(`_nros_features` / `_nros_cpp_features` / `_nros_c_for_cpp_features` at lines
~130/138/148/270/275/282/310/312/314), so a Zephyr `west build` of the C/C++ path
fails at cargo feature resolution.

Not validated/fixed in the convergence work because it needs a west/QEMU Zephyr
build (unavailable in the agent env). Zephyr is already red (#58/#59).

## Remediation (from the C3.2 agent)

In every `set(_nros*_features …)` string:
- `platform-zephyr` → `alloc,global-allocator,critical-section` (append `,std`
  for `CONFIG_BOARD_NATIVE_SIM`/`NATIVE_POSIX`, mirroring the existing
  `nros-rmw-zenoh-staticlib` block ~L173-184).
- `cffi-zenoh-cffi` / `rmw-zenoh-cffi` → drop (zenoh already links standalone via
  the `nros-rmw-zenoh-staticlib` block).
- `cffi-xrce-c` / `rmw-xrce-cffi` → drop, AND add a standalone
  `nros-rmw-xrce-cffi-staticlib` cargo-build block mirroring the zenoh one so
  `nros_rmw_xrce_register` resolves at link (the staticlib crate exists at
  `packages/xrce/nros-rmw-xrce-cffi-staticlib`). Nuance to validate on real
  no_std boards: the xrce staticlib needs a `panic-halt` strategy to compile
  standalone (the ESP-IDF path already does this), reconciled at the final link
  via the existing `-Wl,--allow-multiple-definition`.

Validate with a west build per backend (zenoh / xrce / cyclonedds) on
native_sim + a real board.
