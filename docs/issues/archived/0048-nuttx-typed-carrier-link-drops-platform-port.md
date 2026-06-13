---
id: 48
title: NuttX link drops the platform port (undefined nros_platform_*) — bundled-rlib order
status: resolved
type: bug
area: boards
related: [phase-243, rfc-0042]
resolved_in: phase-243 (nuttx_platform_build.rs — -bundle,+whole-archive)
---

The platform-ci **nuttx** cell failed at **link** (Build ✓): the whole
`nros_platform_*` ABI was undefined when linking the NuttX zenoh image — the
RUST workspace Entry fixture `qemu_nuttx_entry`
(`examples/workspaces/rust`, recipe `just nuttx build-examples` →
`workspace-fixtures-build.sh nuttx rust`):

```
undefined reference to `nros_platform_time_now_ms'  (from libnros_rmw_zenoh)
undefined reference to `nros_platform_alloc'        (from libzpico_sys platform_aliases.o)
… (the whole nros_platform_* surface)
```

## Root cause (corrected)

NOT the typed C/C++ carrier, NOT 240.6, NOT the board `run()` pin — the original
diagnosis was wrong. The platform port (`nros-platform-posix/src/{platform,net}.c`)
is compiled by the **board** crate's build script
(`nros-board-common::nuttx_platform_build::run_platform`) via `cc`, which emitted
the default `cargo:rustc-link-lib=static=nros_platform_nuttx`. `static=` defaults
to **`+bundle`**, so cargo folds the platform objects *into* the consuming rlib
(`libnros_board_nuttx_qemu_arm.rlib`).

On the final link line that rlib precedes the `nros_platform_*` REFERENCERS
(`libnros_rmw_zenoh`, `libzpico_sys`). GNU ld's single archive pass pulls only
members that satisfy *pending* undefineds; at the board-rlib position nothing
references `nros_platform_*` yet, so the bundled platform members are dropped.
The later referencers then have no archive to resolve against ⇒ undefined.
Order-dependent (rlib topo order shifts between commits), hence the intermittent
red. Affects every NuttX binary consuming the board crate (rust Entry + the
C/C++ FFI carrier), header-independent.

## Fix

`nuttx_platform_build.rs`: `cc::Build::cargo_metadata(false)` + emit by hand
`cargo:rustc-link-lib=static:-bundle,+whole-archive=nros_platform_nuttx` (plus the
OUT_DIR search path). `-bundle` keeps the port a standalone `lib*.a` linked as a
trailing `-l` AFTER all rlibs; `+whole-archive` pulls the whole port
unconditionally so `nros_platform_*` are defined before the referencers are
processed — order-independent (RFC-0042 D3). One helper, both NuttX boards
(arm + riscv) and both the rust + C/C++ link paths.

Verified: link line now carries `-Wl,--whole-archive -lnros_platform_nuttx
-Wl,--no-whole-archive` trailing the rlibs; `libnros_board_nuttx_qemu_arm.rlib`
no longer bundles `nros_platform_*` (nm count 0).
