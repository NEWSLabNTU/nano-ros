//! # nros-board-orin-spe
//!
//! Board crate for running nano-ros on the **NVIDIA AGX Orin Sensor
//! Processing Engine (SPE)** — the always-on Cortex-R5F core that boots
//! before the CCPLEX, runs NVIDIA's FreeRTOS V10.4.3 FSP, and survives
//! Linux crashes. The natural home for the autoware_sentinel safety
//! island.
//!
//! # What's distinctive vs other board crates
//!
//! Most nros board crates start the FreeRTOS scheduler themselves
//! (`nros-board-mps2-an385-freertos::run` calls
//! `vTaskStartScheduler`). On the SPE, the **FSP boots the scheduler
//! before user code** — `app_init()` (the hook the FSP calls into) runs
//! inside an already-scheduled task context. This crate's [`run()`]
//! reflects that: it spawns one application task and **returns**, since
//! the scheduler is already up. The firmware's `main()` (linked
//! separately by NVIDIA's Makefile) calls `nros_app_rust_entry()` from
//! `app_init()` and the rest is regular Rust.
//!
//! No Ethernet / lwIP / LAN9118 — the SPE has no network MAC. The only
//! Linux↔SPE transport is **IVC** (Phase 100.4) and the locator default
//! reflects that:
//!
//! ```rust,ignore
//! Config::default().zenoh_locator == "ivc/2"  // channel 2 = aon_echo
//! ```
//!
//! # Build prerequisites
//!
//! - `NV_SPE_FSP_DIR` set to an SDK-Manager-installed FSP tree
//!   (containing `lib/libtegra_aon_fsp.a`).
//! - `armv7r-none-eabihf` rustup target (added to the workspace
//!   pin in Phase 100.2 — `just workspace rust-targets` pulls it).
//! - Nightly toolchain for `-Zbuild-std=core,alloc`.
//!
//! See `README.md` for the full SDK Manager / flash recipe.

#![no_std]
// Force-link the IVC C transport + shim forwarders even when the
// firmware doesn't directly reference them. Without this, the linker
// drops `_z_open_ivc` / `_z_f_link_open_ivc` because nothing in the
// Rust crate graph names them — they're only consumed at the C level.
extern crate nvidia_ivc;
extern crate zpico_link_ivc;
extern crate zpico_sys;

mod config;
mod node;

pub use config::Config;
pub use node::{init_hardware, run};

/// Phase 152.3 — `BoardInit` impl for the AGX Orin SPE.
///
/// Canonical overlay-on-overlay precedent (the SPE is a FreeRTOS
/// fork that runs prebuilt FSP V10.4.3 — no kernel rebuild — and
/// replaces lwIP with IVC). Fits the `nros_board_common::BoardInit`
/// contract documented in `book/src/porting/vendor-overlay.md`
/// so future generic-FreeRTOS overlays (STM32 / NXP / stock-FreeRTOS
/// + lwIP boards) share the same trait shape.
pub struct OrinSpe;

impl nros_board_common::BoardInit for OrinSpe {
    type Config = Config;

    fn init_hardware(cfg: &Config) {
        // Delegate to existing `node::init_hardware` (TCU init +
        // FSP-side bring-up). The IVC channel registration lives
        // inside `run()` since it must follow `tcu_init` ordering
        // — the trait method only handles the pre-run wakes.
        init_hardware(cfg);
    }
}

// ─── Phase 212.N.3 — nros_platform::board trait impls ────────────────────
//
// Additive shim over the new 212.N.1 trait surface. The legacy
// `nros_board_common::BoardInit` impl above stays untouched during
// the transition.
//
// Orin SPE is a kernel-spawn board but with a twist: the FSP boots
// the FreeRTOS scheduler **before** user code runs. `app_init` (the
// FSP hook the firmware crate wraps as `nros_app_rust_entry`) is
// already inside a FreeRTOS task by the time `BoardEntry::run`
// fires. So unlike the generic `nros-board-freertos::run_entry`
// (which calls `vTaskStartScheduler` and never returns), the SPE
// `BoardEntry::run` body drives the user closure directly inside
// the caller's task and diverges via `exit_success`/`exit_failure`
// — both of which halt in `wfi` on real hardware.

impl nros_platform::BoardInit for OrinSpe {
    fn init_hardware() {
        // Parameterless per the 212.N.1 contract. Delegates to
        // `node::init_hardware` with `Config::default()` because
        // the SPE's `init_hardware` body is a no-op anyway (FSP
        // already brought up TCU / HSP / IVC carveout by the time
        // user code runs). The arg exists only for API parity
        // with other board crates.
        init_hardware(&Config::default());
    }
}

impl nros_platform::BoardPrint for OrinSpe {
    fn println(args: core::fmt::Arguments<'_>) {
        // Stage into the same 256-byte stack buffer the macro
        // `println!` uses, then forward to FSP's TCU printf via
        // `__fsp_println` so we keep one path for all output.
        let mut buf = __PrintBuf::new();
        let _ = core::fmt::Write::write_fmt(&mut buf, args);
        __fsp_println(buf.as_str());
    }
}

impl nros_platform::BoardExit for OrinSpe {
    fn exit_success() -> ! {
        // The SPE is the always-on safety core — there is no
        // "exit cleanly to a host" path. Park in `wfi`. The FSP's
        // idle hook keeps firing; the firmware's downstream
        // watchdog logic decides whether to reset the SoC.
        loop {
            unsafe {
                core::arch::asm!("wfi", options(nomem, nostack, preserves_flags));
            }
        }
    }

    fn exit_failure() -> ! {
        loop {
            unsafe {
                core::arch::asm!("wfi", options(nomem, nostack, preserves_flags));
            }
        }
    }
}

impl nros_platform::BoardEntry for OrinSpe {
    /// Drive the boot → setup → exit flow on AGX Orin SPE.
    ///
    /// **Already inside a FreeRTOS task** when invoked — the FSP's
    /// `app_init` hook spawns the task and trampolines into the
    /// firmware's `nros_app_rust_entry`, which is where the Entry
    /// pkg's `main`-equivalent calls `<OrinSpe as BoardEntry>::run`.
    ///
    /// Body shape (mirrors `nros-board-posix` but with the SPE
    /// banner + the wfi-halt exit pair):
    ///
    /// 1. Print the same banner as the legacy `node::run`.
    /// 2. [`nros_platform::BoardInit::init_hardware`] (no-op on SPE).
    /// 3. Build [`nros_platform::RuntimeCtx`] via `with_runtime`; codegen
    ///    (212.N.4, lives in standalone `nros-cli` repo per
    ///    CLAUDE.md) will populate `params` / `remaps` later.
    /// 4. Invoke `setup(&mut runtime)`.
    /// 5. Diverge via `exit_success` / `exit_failure`.
    ///
    /// The legacy [`run`] free fn (which `xTaskCreate`s a fresh
    /// task and returns into `app_init`) coexists during the
    /// 212.N transition; existing firmware that calls `run()`
    /// keeps working.
    fn run<F, E>(setup: F) -> Result<(), E>
    where
        F: FnOnce(&mut nros_platform::RuntimeCtx<'_>) -> Result<(), E>,
        E: core::fmt::Debug,
    {
        use nros_platform::{BoardExit, BoardInit, BoardPrint};

        <Self as BoardPrint>::println(format_args!(""));
        <Self as BoardPrint>::println(format_args!("========================================"));
        <Self as BoardPrint>::println(format_args!("  nros-board-orin-spe (Cortex-R5F)"));
        <Self as BoardPrint>::println(format_args!("========================================"));
        <Self as BoardPrint>::println(format_args!(""));

        <Self as BoardInit>::init_hardware();

        // Phase 212.N.7 step-3.2 — placeholder runtime; step-3.5 wires
        // the real `ExecutorNodeRuntime`.
        let mut crt = nros_platform::NullNodeRuntime;
        let mut runtime = nros_platform::RuntimeCtx::with_runtime(&mut crt);
        match setup(&mut runtime) {
            Ok(()) => {
                <Self as BoardPrint>::println(format_args!(""));
                <Self as BoardPrint>::println(format_args!(
                    "nros-board-orin-spe: application closure returned Ok."
                ));
                <Self as BoardExit>::exit_success();
            }
            Err(e) => {
                <Self as BoardPrint>::println(format_args!(
                    "nros-board-orin-spe: application error: {e:?}"
                ));
                <Self as BoardExit>::exit_failure();
            }
        }
    }
}

/// Print to the SPE's TCU (Tegra Combined UART) via FSP's `printf`.
///
/// On real hardware this writes to the shared SoC debug UART; on the
/// `unix-mock` host build it falls back to libc `printf` (stdout).
///
/// `printf` is a varargs C function — for `no_std` we route every call
/// through `core::fmt::write` into a fixed-size stack buffer, then
/// pass that buffer as a `%s` argument so we don't depend on Rust's
/// formatting machinery touching libc.
#[macro_export]
macro_rules! println {
    () => {
        $crate::__fsp_println("");
    };
    ($($arg:tt)*) => {{
        let mut buf = $crate::__PrintBuf::new();
        let _ = core::fmt::Write::write_fmt(&mut buf, core::format_args!($($arg)*));
        $crate::__fsp_println(buf.as_str());
    }};
}

/// Stack buffer used by [`println!`]. 256 bytes covers a typical
/// banner / status line; longer messages are silently truncated, which
/// is the right behaviour for a debug-only logger that must not
/// allocate on the SPE's 256 KB BTCM.
#[doc(hidden)]
pub struct __PrintBuf {
    buf: [u8; 256],
    len: usize,
}

impl __PrintBuf {
    pub const fn new() -> Self {
        Self {
            buf: [0; 256],
            len: 0,
        }
    }

    pub fn as_str(&self) -> &str {
        // SAFETY: bytes 0..len were written via `write_str` which
        // accepts `&str` only, so they're valid UTF-8.
        unsafe { core::str::from_utf8_unchecked(&self.buf[..self.len]) }
    }
}

impl Default for __PrintBuf {
    fn default() -> Self {
        Self::new()
    }
}

impl core::fmt::Write for __PrintBuf {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        // Reserve 1 byte for trailing NUL so `as_str()` callers that
        // pass through `as_ptr()` to a C printf still see a
        // null-terminated string. We never expose those bytes through
        // `as_str()`'s slice end.
        let avail = self.buf.len() - self.len - 1;
        let to_copy = s.len().min(avail);
        self.buf[self.len..self.len + to_copy].copy_from_slice(&s.as_bytes()[..to_copy]);
        self.len += to_copy;
        Ok(())
    }
}

#[doc(hidden)]
pub fn __fsp_println(s: &str) {
    // Route through `tcu_print_msg(buf, len, from_isr)` — FSP's raw TCU
    // writer — instead of newlib `printf`. printf pulls vfprintf +
    // _dtoa_r + __jp2uc + 128-bit float intrinsics (fmaf128, __divtf3,
    // __addtf3, __multf3, lgamma_r, fmodf128, …) through long-double
    // %f support, costing ~25 KB .text on the Cortex-R5F's 256 KB BTCM
    // even when no caller ever prints a float. tcu_print_msg is a
    // straight memcpy-into-FIFO loop; the appended '\n' is the only
    // formatting we ever do here.
    unsafe extern "C" {
        fn tcu_print_msg(
            msg_buf: *const core::ffi::c_char,
            len: core::ffi::c_int,
            from_isr: bool,
        ) -> core::ffi::c_int;
    }
    let bytes = s.as_bytes();
    if !bytes.is_empty() {
        unsafe {
            tcu_print_msg(
                bytes.as_ptr() as *const core::ffi::c_char,
                bytes.len() as i32,
                false,
            );
        }
    }
    const NL: &[u8] = b"\n";
    unsafe {
        tcu_print_msg(NL.as_ptr() as *const core::ffi::c_char, 1, false);
    }
}
