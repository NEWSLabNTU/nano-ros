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

## Step 1 DONE (2026-07-16) — `cyclonedds_app.c` strong overrides retired

All 6 examples' hand-written strong
`nros_rmw_cyclonedds_register_app_descriptors` overrides are deleted (the TU
stays as the empty link-anchor only). Verified:

- All 6 `build-cyclonedds` images rebuild + link clean (the weak no-op in
  `vtable.cpp` resolves; `nm` shows `W nros_rmw_cyclonedds_register_app_descriptors`).
- The #195 board `.init_array` walk fires in the RUST images too
  (`[board] Running .init_array constructors...` in the boot log), so the
  idlc-generated ctor TUs register the descriptors.
- Runtime: the rust cyclone talker creates its publisher and publishes 42
  samples in a two-QEMU dgram boot (pre-#195 this failed create with -1) —
  descriptor registration is ctor-driven, no hand shim needed.

**Found en route (pre-existing, this lane has NO test consumer —
`build_threadx_rv64_rust_example_rmw` is defined but nothing calls it):**
- The rust examples' deploy blocks bake `domain_id = 0` while the C cyclone
  pair (and the #195 test) run domain 62 → a rust↔C pair can never discover.
- The rust pair shares the board-default firmware MAC (the deploy overlay
  carries no `mac` field — same latent hazard noted on esp32 in #190), so a
  rust↔rust two-QEMU pair on one L2 link fails identity/ARP → 0 delivery.
Both belong to steps 2–4's cleanup (or a follow-up demo/e2e wave): fix the
domain bake, differentiate the MAC, and add a consumer test when the lane is
made real.

Steps 2–4 (app_main trampoline → board crate/macro; link anchors; shared
CMake seam) remain open.
