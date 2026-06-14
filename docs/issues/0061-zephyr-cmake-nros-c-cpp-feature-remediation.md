---
id: 61
title: zephyr/CMakeLists.txt passes removed nros-c/nros-cpp features (phase-248 C3.2 downstream)
status: open
type: bug
area: zephyr
related: [phase-248, issue-0060, issue-0058, issue-0059]
---

## The break

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
