---
id: 205
title: "qemu-riscv64-threadx rust examples carry framework boilerplate: hand-written cyclonedds_app.c, app_main FFI shim, full CMake wiring, link anchors"
status: open
type: tech-debt
area: examples
related: [issue-0195, rfc-0026]
---

## Problem (audit 2026-07-16, J1)

All 6 `examples/qemu-riscv64-threadx/rust/*` copy-out examples ship glue a
user should never copy:

1. `src/cyclonedds_app.c` — strong override of the weak
   `nros_rmw_cyclonedds_register_app_descriptors` calling
   `register_std_msgs_*_0()` by hand. Since #195 landed the board
   `.init_array` walk, the generated ctor-based register TUs likely make this
   TU retirable outright (verify on the rust lane like the C lane).
2. `src/lib.rs` `#[unsafe(no_mangle)] pub extern "C" fn app_main()` trampoline
   into `board::run_app_thread` — the board crate / `nros::main!` should emit
   the C-ABI entry.
3. `CMakeLists.txt` with corrosion import + `nros_generate_interfaces` +
   root `add_subdirectory` + RMW link — no other rust example family carries
   a CMakeLists; belongs in a reusable cmake function / board module.
4. `extern crate nros_platform_critical_section as _` link anchors in the
   node lib (staticlib DCE workaround) — framework linkage guarantee.

## Fix sketch

Retire (1) first (test with the init_array walk), then move (2)+(4) into the
board crate/macro, then collapse (3) into a shared cmake seam. Each step
keeps the rust riscv64-threadx e2e lane green.
