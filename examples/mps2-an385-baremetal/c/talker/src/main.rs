//! The startup chain for the C talker beside it (issue 1512).
//!
//! `<nros/app_main.h>` has always said what bare metal needs: on this platform
//! `NROS_APP_MAIN_REGISTER()` emits `void app_main(void)`, and "per-platform
//! startup chains call this after platform init (network, executor arena, board
//! hw)". On every RTOS that chain is C. Here it cannot be, and the reason is
//! not the C API — it is that on a board with no RTOS the port IS the board,
//! and this board's port is Rust:
//!
//! * the reset vector and the `.data`/`.bss` init come from `cortex-m-rt`;
//! * the monotonic clock is CMSDK Timer0, driven from
//!   `nros_platform_mps2_an385::clock`;
//! * the network is a LAN9118 behind `smoltcp`, brought up by
//!   `nros_board_mps2_an385::init_hardware` and reachable from C only AFTER
//!   that call, through the `nros_platform_*` CFFI exports;
//! * the console is semihosting, and the `nros_log` sink list is published by
//!   the board's boot funnel.
//!
//! None of that has a C entry point, so the link root is Rust and the C
//! application is compiled into it (`build.rs`). That inverts the FreeRTOS /
//! NuttX / ThreadX shape, where the RTOS supplies a C startup and `libnros_c.a`
//! is what gets linked IN.
//!
//! What this file is not: it is not a second boot funnel. `run_bare` is the
//! board's own no-session funnel (`nros_board_mps2_an385::run_bare`), the same
//! one the no-alloc benches use; everything here happens inside it.

#![no_std]
#![no_main]

use nros_board_mps2_an385::{Config, entry, run_bare};

unsafe extern "C" {
    /// Emitted by `NROS_APP_MAIN_REGISTER()` in `src/talker.c`.
    fn app_main();
}

/// Keep `nros-c`'s `#[no_mangle]` C surface in the link.
///
/// `nros-c` is an RLIB here, not the staticlib the cmake road imports, and this
/// board links with `--gc-sections` (the board descriptor's rustflags). Nothing
/// in this crate's Rust references `nros_support_init` and friends — only the C
/// object does, and it is an archive member the linker reaches later — so
/// without a live reference from the binary's own code the whole surface is
/// dropped and every C call becomes an undefined symbol.
///
/// `C_SURFACE_ANCHOR` is `nros-c`'s own generated `#[used]` array of its ungated
/// entry points, built for exactly this class of problem ("so the entry points
/// survive DCE when `nros-c` is bundled as an rlib"). Referencing it here is the
/// binary-root half of that arrangement.
fn anchor_c_surface() {
    core::hint::black_box(&nros_c::c_surface_anchor::C_SURFACE_ANCHOR);
}

#[entry]
fn main() -> ! {
    anchor_c_surface();

    // NOTE: the backend is deliberately NOT registered here. Bare metal runs no
    // `.init_array` and the linkme section is never walked, so registration is
    // explicit — and the hook `nros_support_init` itself calls is the C
    // `nros_app_register_backends()`, which `src/talker.c` defines the way
    // `nano_ros_link_rmw()` generates it on the cmake road. A
    // `nros_rmw_zenoh::register()` here would be idempotent and harmless, and it
    // would also be a second site for one fact.

    // Clock + LAN9118/smoltcp bring-up, the `nros_log` sink list, then us.
    run_bare(Config::default(), |_cfg| {
        // SAFETY: `app_main` is the C entry the header's own macro emits; it
        // takes no arguments and returns nothing. The board is up by the time
        // `run_bare` calls this closure, which is the ordering the C side
        // assumes and cannot check.
        unsafe { app_main() };
        Ok::<(), core::convert::Infallible>(())
    })
}
