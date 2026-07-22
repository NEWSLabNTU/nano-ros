//! # nros-board-threadx-linux
//!
//! Board crate for running nros on Linux with ThreadX + NetX Duo.
//!
//! ThreadX runs as pthreads via its Linux simulation port, and NetX Duo
//! uses a raw-socket Linux driver for real Ethernet over TAP interfaces.
//! This mirrors the FreeRTOS board crate pattern but is simpler since the
//! host kernel provides POSIX sockets (NSOS shim) instead of a full
//! NetX-Duo TCP/IP stack.
//!
//! Users call [`run()`] with a closure that receives `&Config` and creates
//! an `Executor` for full API access (publishers, subscriptions, services,
//! actions, timers, callbacks).
//!
//! # `no_std`
//!
//! Phase 152.4.B prep — this crate is `no_std`. ThreadX-Linux runs in
//! user space against libc but does not pull `std`; the few libc
//! entry points it needs (`exit`, `fputs`) are declared as bare
//! externs. Drops the `std` blocker for the future generic
//! `nros_board_threadx::run<B>` lift (152.2.B.4).

#![no_std]

mod config;
mod node;

pub use config::Config;
pub use node::{init_hardware, run};

use nros_board_common::{BoardExit, BoardInit, BoardPrint, ThreadxConfig};

/// Per-board marker for trait dispatch.
pub struct ThreadxLinux;

impl BoardInit for ThreadxLinux {
    type Config = Config;

    fn init_hardware(cfg: &Config) {
        init_hardware(cfg);
    }
}

impl ThreadxConfig for Config {
    fn mac(&self) -> &[u8; 6] {
        &self.mac
    }
    fn ip(&self) -> &[u8; 4] {
        &self.ip
    }
    fn netmask(&self) -> &[u8; 4] {
        &self.netmask
    }
    fn gateway(&self) -> &[u8; 4] {
        &self.gateway
    }
    fn interface(&self) -> Option<&str> {
        Some(self.interface)
    }
    fn zenoh_locator(&self) -> &'static str {
        self.zenoh_locator
    }
    fn domain_id(&self) -> u32 {
        self.domain_id
    }
}

impl BoardPrint for ThreadxLinux {
    fn println(args: core::fmt::Arguments<'_>) {
        // Stage into a fixed stack buffer + push NUL, then forward
        // through the shared C-side `nros_board_log` FFI (already
        // wired to libc `fputs(stdout)` in `c/board_threadx_linux.c`).
        // Drops the `std` `println!` dependency without adding any
        // new FFI surface.
        use core::fmt::Write;
        let mut buf = NulBuf::<512>::new();
        let _ = writeln!(buf, "{}", args);
        unsafe extern "C" {
            fn nros_board_log(s: *const u8);
        }
        unsafe { nros_board_log(buf.as_nul_terminated_ptr()) };
    }
}

impl BoardExit for ThreadxLinux {
    fn exit_success() -> ! {
        unsafe { libc_exit(0) }
    }

    fn exit_failure() -> ! {
        unsafe { libc_exit(1) }
    }
}

// ── Phase 212.N.3 — `nros_platform::board` trait impls + `BoardEntry` ────
//
// Additive overlay on top of the legacy `nros_board_common::Board*`
// impls above. The new 212.N.1 trait set lives in
// `nros_platform::board::*` (`BoardInit` parameterless, `BoardPrint`,
// `BoardExit`, plus the `BoardEntry` boot driver consumed by user
// `main.rs`). The legacy traits stay for now (per CLAUDE.md /
// `nros_platform::board` module docs — Phase 212.N.7 retires them).
//
// `BoardEntry::run` body delegates to the family driver in
// `nros-board-threadx::run_entry::<ThreadxLinux, Config, F, E>`,
// which mirrors the legacy `run` lifecycle (stash closure, register
// network config + app callback, `tx_kernel_enter()`).

impl nros_platform::BoardInit for ThreadxLinux {
    // New 212.N.1 init is parameterless. The threadx-linux overlay's
    // pre-kernel init is a no-op (see `node::init_hardware` — actual
    // network bring-up runs in `tx_application_define()` after the
    // kernel starts), so calling through to it with the default
    // config is safe; the `_config: &Config` arg is ignored.
    fn init_hardware() {
        crate::node::init_hardware(&Config::default());
    }
}

impl nros_platform::BoardPrint for ThreadxLinux {
    fn println(args: core::fmt::Arguments<'_>) {
        // Delegate to the legacy `BoardPrint` impl — same staging
        // buffer + `nros_board_log` FFI path.
        <Self as BoardPrint>::println(args);
    }
}

impl nros_platform::BoardExit for ThreadxLinux {
    fn exit_success() -> ! {
        unsafe { libc_exit(0) }
    }

    fn exit_failure() -> ! {
        unsafe { libc_exit(1) }
    }
}

/// Issue #194 — line-buffer stdout before any output. The Rust Entry image's
/// `nros::main!` `fn main` overrides the board `startup.c` weak C `main` (which
/// carried the `setvbuf` fix), so when a test harness pipes stdout glibc
/// full-buffers it and the readiness banner never reaches the harness within
/// its gate window. Mirrors `startup.c` / the C examples' `_IOLBF` fix.
fn line_buffer_stdout() {
    unsafe extern "C" {
        fn setvbuf(
            stream: *mut core::ffi::c_void,
            buffer: *mut core::ffi::c_char,
            mode: core::ffi::c_int,
            size: usize,
        ) -> core::ffi::c_int;
        // glibc's stdout FILE* — hosted Linux only (this crate is the
        // ThreadX-on-Linux simulation board, so that is always true here).
        static mut stdout: *mut core::ffi::c_void;
    }
    const IOLBF: core::ffi::c_int = 1; // glibc _IOLBF
    unsafe {
        setvbuf(stdout, core::ptr::null_mut(), IOLBF, 0);
    }
}

impl nros_platform::BoardEntry for ThreadxLinux {
    fn run<F, E>(setup: F) -> Result<(), E>
    where
        F: FnOnce(&mut nros_platform::RuntimeCtx<'_>) -> Result<(), E>,
        E: core::fmt::Debug,
    {
        // Refresh the platform log slot before kernel entry (mirrors
        // the legacy `node::run` wrapper). The ThreadX-Linux kernel
        // bring-up resets some C static state, so `node::run` also
        // re-registers from inside the app thread; here we just seed
        // the slot for any pre-kernel logging.
        line_buffer_stdout();
        crate::node::register_log_writer_public();
        let cfg = Config::default();
        nros_board_threadx::run_entry::<ThreadxLinux, Config, F, E>(cfg, None, setup)
    }

    /// Issue #48 cause 1 / Phase 244 E5 — apply the `nros::main!()` deploy overlay
    /// (`[package.metadata.nros.deploy.threadx-linux]`: locator / ip / gateway /
    /// netmask / domain_id) onto `Config::default()` before boot, so the firmware
    /// dials the deploy-named endpoint instead of the inert compiled-in default.
    /// Fields the deploy block omits keep the board default. (NSOS routes through
    /// the host kernel, so `locator`/`domain_id` are the load-bearing fields here.)
    ///
    /// Issue #98 / RFC-0045 — also threads `deploy.boot_config` so the node name
    /// comes from the baked `.nros_boot_config`.
    fn run_with_deploy<F, E>(deploy: &nros_platform::DeployOverlay, setup: F) -> Result<(), E>
    where
        F: FnOnce(&mut nros_platform::RuntimeCtx<'_>) -> Result<(), E>,
        E: core::fmt::Debug,
    {
        line_buffer_stdout();
        crate::node::register_log_writer_public();
        nros_board_threadx::run_entry::<ThreadxLinux, Config, F, E>(
            config_with_overlay(deploy),
            deploy.boot_config,
            setup,
        )
    }
}

impl ThreadxLinux {
    /// Phase 297 W4 (RFC-0053) — multi-tier entry. The `nros::main!()` macro
    /// emits `<ThreadxLinux>::run_tiers(&overlay, TIERS, setup)` whenever a
    /// system declares more than the synthesized single `default` tier; this
    /// routes to [`nros_board_threadx::run_tiers_entry`], which runs one
    /// `Executor` per tier over one shared session. Mirrors
    /// `Mps2An385::run_tiers` (the FreeRTOS analogue).
    pub fn run_tiers<F, E>(
        deploy: &nros_platform::DeployOverlay,
        tiers: &'static [nros_platform::TierSpec<'static>],
        setup: F,
    ) -> Result<(), E>
    where
        F: Fn(&mut nros_platform::RuntimeCtx<'_>) -> Result<(), E> + Copy,
        E: core::fmt::Debug,
    {
        line_buffer_stdout();
        crate::node::register_log_writer_public();
        nros_board_threadx::run_tiers_entry::<ThreadxLinux, Config, F, E>(
            config_with_overlay(deploy),
            deploy.boot_config,
            tiers,
            setup,
        )
    }
}

/// Phase 244 E5 — overlay the `nros::main!()` deploy block onto `Config::default()`.
/// Fields the deploy block omits keep the board default.
fn config_with_overlay(deploy: &nros_platform::DeployOverlay) -> Config {
    let mut config = Config::default();
    if let Some(loc) = deploy.locator {
        config.zenoh_locator = loc;
    }
    if let Some(ip) = deploy.ip {
        config.ip = ip;
    }
    if let Some(gw) = deploy.gateway {
        config.gateway = gw;
    }
    if let Some(nm) = deploy.netmask {
        config.netmask = nm;
    }
    if let Some(d) = deploy.domain_id {
        config.domain_id = d;
    }
    config
}

unsafe fn libc_exit(code: i32) -> ! {
    unsafe extern "C" {
        fn exit(status: i32) -> !;
    }
    unsafe { exit(code) }
}

/// Stack buffer that yields a NUL-terminated `*const u8` for libc
/// consumers. Truncates on overflow rather than allocating.
pub(crate) struct NulBuf<const N: usize> {
    buf: [u8; N],
    len: usize,
}

impl<const N: usize> NulBuf<N> {
    pub(crate) const fn new() -> Self {
        Self {
            buf: [0; N],
            len: 0,
        }
    }

    /// Returns a pointer to a NUL-terminated copy of the written
    /// bytes. The final byte is always NUL — overflow truncates
    /// the message, not the terminator.
    pub(crate) fn as_nul_terminated_ptr(&mut self) -> *const u8 {
        if self.len < N {
            self.buf[self.len] = 0;
        } else {
            self.buf[N - 1] = 0;
        }
        self.buf.as_ptr()
    }
}

impl<const N: usize> core::fmt::Write for NulBuf<N> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let bytes = s.as_bytes();
        // Reserve 1 byte for NUL.
        let avail = N.saturating_sub(self.len + 1);
        let take = bytes.len().min(avail);
        self.buf[self.len..self.len + take].copy_from_slice(&bytes[..take]);
        self.len += take;
        Ok(())
    }
}
