//! The startup chain for the C application beside it (issue 1512).
//!
//! LEAF-AGNOSTIC, like `build.rs`: every C leaf in this family carries these
//! same bytes, and differs only in `package.xml` and its own `src/*.c`.
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
    /// The app's entry, defined in the leaf's `src/*.c` beside its
    /// `NROS_APP_MAIN_REGISTER()`.
    fn nros_app_main(argc: i32, argv: *mut *mut core::ffi::c_char) -> i32;
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
    // `nros_app_register_backends()`, which the leaf's `src/*.c` defines the way
    // `nano_ros_link_rmw()` generates it on the cmake road. A
    // `nros_rmw_zenoh::register()` here would be idempotent and harmless, and it
    // would also be a second site for one fact.

    // Clock + LAN9118/smoltcp bring-up, the `nros_log` sink list, then us.
    //
    // Issue 1770 — the app's STATUS decides how QEMU exits. This used to call
    // the macro's `void app_main(void)`, which has nowhere to return a status,
    // and then return `Ok`, so a failed app exited QEMU with success. Calling
    // `nros_app_main` is the `int main` shim's shape (`<nros/app_main.h>`): no
    // weak/strong recorder pair, so nothing depends on which archive member
    // the linker happens to load. `run_bare` maps `Err` to `exit_failure`.
    run_bare(Config::default(), |_cfg| {
        // SAFETY: `nros_app_main` is the C entry every leaf defines; `argc = 0`
        // with a null `argv` is what the header's own VOID shim passes. The
        // board is up by the time `run_bare` calls this closure, which is the
        // ordering the C side assumes and cannot check.
        let status = unsafe { nros_app_main(0, core::ptr::null_mut()) };
        if status == 0 { Ok(()) } else { Err(status) }
    })
}
