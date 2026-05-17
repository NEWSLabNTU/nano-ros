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
    fn mac(&self) -> &[u8; 6] { &self.mac }
    fn ip(&self) -> &[u8; 4] { &self.ip }
    fn netmask(&self) -> &[u8; 4] { &self.netmask }
    fn gateway(&self) -> &[u8; 4] { &self.gateway }
    fn interface(&self) -> Option<&str> { Some(self.interface) }
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
        Self { buf: [0; N], len: 0 }
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
