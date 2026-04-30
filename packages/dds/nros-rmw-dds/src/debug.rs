//! Phase 97.4 bring-up trace macros — gated behind the cargo `debug-*`
//! features so production builds compile away to nothing. Used by both
//! `transport_nros.rs` (participant create / port-bind path) and
//! `session.rs` (DDS publisher / subscriber factory round-trips).
//!
//! Three independent backends so every platform we ship has a working
//! trace channel:
//! - `debug-cortex-m-semihosting` — Cortex-M (MPS2-AN385, STM32F4)
//! - `debug-stderr`               — std-capable (NuttX, ThreadX-Linux,
//!                                    Zephyr native_sim)
//! - `debug-uart`                 — no_std + no semihosting (ThreadX
//!                                    on QEMU RISC-V, ESP32 bare-metal)

#![cfg(feature = "alloc")]

#[cfg(all(
    feature = "debug-uart",
    not(feature = "debug-cortex-m-semihosting"),
    not(feature = "debug-stderr"),
))]
pub(crate) mod debug_uart {
    extern crate alloc;
    use alloc::format;
    use core::fmt::Write;

    /// Char-at-a-time UART putter provided by the board crate.
    /// Linker resolves the symbol when `feature = "debug-uart"` is on.
    unsafe extern "C" {
        pub fn uart_putc(c: u8);
    }

    pub struct UartWriter;
    impl Write for UartWriter {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            for byte in s.bytes() {
                unsafe { uart_putc(byte) };
            }
            Ok(())
        }
    }

    pub fn write(s: &str) {
        let mut w = UartWriter;
        let _ = write!(w, "{}", s);
    }

    pub fn writeln_args(args: core::fmt::Arguments<'_>) {
        let s = format!("[nros-rmw-dds] {args}\n");
        write(&s);
    }
}

#[cfg(feature = "debug-cortex-m-semihosting")]
#[macro_export]
macro_rules! dbg_log {
    ($($arg:tt)*) => {
        cortex_m_semihosting::hprintln!("[nros-rmw-dds] {}", format_args!($($arg)*));
    };
}

// `extern crate std` for the `debug-stderr` arm lives at lib.rs so
// the macro's `std::println!` resolves at every call site.

#[cfg(all(feature = "debug-stderr", not(feature = "debug-cortex-m-semihosting")))]
#[macro_export]
macro_rules! dbg_log {
    ($($arg:tt)*) => {
        // Despite the feature name, route through stdout so test
        // harnesses that drain only `child.stdout` see these traces.
        ::std::println!("[nros-rmw-dds] {}", format_args!($($arg)*));
    };
}

#[cfg(all(
    feature = "debug-uart",
    not(feature = "debug-cortex-m-semihosting"),
    not(feature = "debug-stderr"),
))]
#[macro_export]
macro_rules! dbg_log {
    ($($arg:tt)*) => {
        $crate::debug::debug_uart::writeln_args(format_args!($($arg)*));
    };
}

#[cfg(all(
    not(feature = "debug-cortex-m-semihosting"),
    not(feature = "debug-stderr"),
    not(feature = "debug-uart"),
))]
#[macro_export]
macro_rules! dbg_log {
    ($($arg:tt)*) => {{
        let _ = format_args!($($arg)*);
    }};
}
