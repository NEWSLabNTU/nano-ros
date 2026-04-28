//! # nros-board-threadx-qemu-riscv64
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

// Boot entry point — placed in .text.init so the linker script puts it first.
// Must be in the binary crate's compilation unit (not a library) for LLD to
// place it before other .text sections.
core::arch::global_asm!(
    r#"
.section .text.init
.align 4
.global _start
.extern main
.extern _sysstack_start
.extern _bss_start
.extern _bss_end
_start:
    csrr t0, mhartid
    bne  t0, zero, 1f
    li x1, 0
    li x2, 0
    li x3, 0
    li x4, 0
    li x5, 0
    li x6, 0
    li x7, 0
    li x8, 0
    li x9, 0
    li x10, 0
    li x11, 0
    li x12, 0
    li x13, 0
    li x14, 0
    li x15, 0
    li x16, 0
    li x17, 0
    li x18, 0
    li x19, 0
    li x20, 0
    li x21, 0
    li x22, 0
    li x23, 0
    li x24, 0
    li x25, 0
    li x26, 0
    li x27, 0
    li x28, 0
    li x29, 0
    li x30, 0
    li x31, 0
    la t0, _sysstack_start
    li t1, 0x1000
    add sp, t0, t1
    la  t0, _bss_start
    la  t1, _bss_end
2:
    bgeu t0, t1, 3f
    sb zero, 0(t0)
    addi t0, t0, 1
    j 2b
3:
    call main
1:
    wfi
    j 1b
"#
);

pub use config::Config;
pub use node::{init_hardware, run};

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
