//! # nros-threadx-qemu-riscv64
//!
//! Board crate for running nros on QEMU RISC-V 64-bit virt with
//! ThreadX + NetX Duo + virtio-net.
//!
//! Handles board init (PLIC, UART, CLINT timer), ThreadX kernel startup,
//! NetX Duo IP stack (with virtio-net Ethernet), and spawns the
//! application thread that calls back into Rust.
//!
//! Users call [`run()`] with a closure that receives `&Config` and creates
//! an `Executor` for full API access (publishers, subscriptions, services,
//! actions, timers, callbacks).

#![no_std]

mod config;
mod node;

pub use config::Config;
pub use node::run;

/// Print to QEMU UART console.
///
/// Uses the 16550 UART at `0x10000000` (QEMU virt machine).
/// Calls the C `uart_puts()` function compiled by `build.rs`.
#[macro_export]
macro_rules! println {
    () => {
        $crate::uart_write_str("\n")
    };
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        let mut buf = $crate::UartWriter;
        let _ = core::writeln!(buf, $($arg)*);
    }};
}

/// UART writer for `core::fmt::Write`.
pub struct UartWriter;

impl core::fmt::Write for UartWriter {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        uart_write_str(s);
        Ok(())
    }
}

/// Write a string to the UART.
pub fn uart_write_str(s: &str) {
    unsafe extern "C" {
        fn uart_putc(c: u8);
    }
    for byte in s.bytes() {
        unsafe { uart_putc(byte) };
    }
}

/// Exit QEMU via test-finisher MMIO device.
///
/// QEMU virt machine `test-finisher` at `0x100000`:
/// write `0x5555` for success (PASS), `0x3333` for failure (FAIL).
pub fn exit_success() -> ! {
    unsafe {
        core::ptr::write_volatile(0x100000 as *mut u32, 0x5555);
    }
    #[allow(clippy::empty_loop)]
    loop {
        core::hint::spin_loop();
    }
}

/// Exit QEMU with failure status.
pub fn exit_failure() -> ! {
    unsafe {
        core::ptr::write_volatile(0x100000 as *mut u32, 0x3333);
    }
    #[allow(clippy::empty_loop)]
    loop {
        core::hint::spin_loop();
    }
}

/// Panic handler — prints message and exits QEMU.
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    uart_write_str("PANIC: ");
    {
        use core::fmt::Write;
        let mut buf = UartWriter;
        let _ = write!(buf, "{}", info);
    }
    uart_write_str("\n");
    exit_failure()
}
