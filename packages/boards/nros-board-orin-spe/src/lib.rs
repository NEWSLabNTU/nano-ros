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
extern crate zpico_platform_shim;
extern crate zpico_sys;

mod config;
mod node;

pub use config::Config;
pub use node::{init_hardware, run};

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
        Self { buf: [0; 256], len: 0 }
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
    // FSP exposes plain `printf` from its newlib build. We append a
    // trailing newline + NUL inside `__PrintBuf::as_str` already — no:
    // the buffer is non-NUL; we pass it as `%.*s`-style by length to
    // avoid relying on a NUL terminator.
    unsafe extern "C" {
        fn printf(fmt: *const core::ffi::c_char, ...) -> core::ffi::c_int;
    }
    // `%.*s\n` lets us pass length + ptr explicitly. Format string is
    // a static byte string with NUL — safe to take as `*const c_char`.
    const FMT: &[u8] = b"%.*s\n\0";
    unsafe {
        printf(
            FMT.as_ptr() as *const core::ffi::c_char,
            s.len() as core::ffi::c_int,
            s.as_ptr(),
        );
    }
}
