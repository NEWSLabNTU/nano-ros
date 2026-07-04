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

// Keep `nros-platform` linked for Rust allocator registration. The
// board's Cargo feature set enables `global-allocator`, which routes
// `alloc` through the ThreadX C platform byte pool.
extern crate nros_platform as _;

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

// Phase 152.2.B — canonical overlay trait impls. Generic-crate
// `run<B>` lift deferred; trait surface lands now.
use nros_board_common::{BoardExit, BoardInit, BoardPrint, ThreadxConfig};

/// Per-board marker for trait dispatch.
pub struct ThreadxQemuRiscv64;

impl BoardInit for ThreadxQemuRiscv64 {
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
    // No host interface — bare-metal NetX-Duo + virtio-net.

    fn zenoh_locator(&self) -> &'static str {
        self.zenoh_locator
    }
    fn domain_id(&self) -> u32 {
        self.domain_id
    }
}

impl BoardPrint for ThreadxQemuRiscv64 {
    fn println(args: core::fmt::Arguments<'_>) {
        use core::fmt::Write;
        let mut w = UartWriter;
        let _ = writeln!(w, "{}", args);
    }
}

impl BoardExit for ThreadxQemuRiscv64 {
    fn exit_success() -> ! {
        exit_success()
    }

    fn exit_failure() -> ! {
        exit_failure()
    }
}

// Phase 212.N.3 — 212.N.1 trait surface (`nros_platform::board::*`)
// + `BoardEntry` delegating to the family driver
// `nros_board_threadx::run_entry`. Additive to the legacy
// `nros-board-common` impls above; both shapes coexist during the
// 212.N migration.
//
// `BoardInit::init_hardware()` (212.N.1) is parameterless — the
// existing `node::init_hardware` already ignores its `&Config` arg, so
// the new impl just forwards through a default Config. `BoardPrint`
// / `BoardExit` mirror the legacy bodies. `BoardEntry::run` constructs
// `Config::default()` and hands it to `nros_board_threadx::run_entry`
// (which threads the closure through ThreadX kernel startup +
// `RuntimeCtx`).
impl nros_platform::BoardInit for ThreadxQemuRiscv64 {
    fn init_hardware() {
        let cfg = Config::default();
        node::init_hardware(&cfg);
    }
}

impl nros_platform::BoardPrint for ThreadxQemuRiscv64 {
    fn println(args: core::fmt::Arguments<'_>) {
        use core::fmt::Write;
        let mut w = UartWriter;
        let _ = writeln!(w, "{}", args);
    }
}

impl nros_platform::BoardExit for ThreadxQemuRiscv64 {
    fn exit_success() -> ! {
        exit_success()
    }

    fn exit_failure() -> ! {
        exit_failure()
    }
}

impl nros_platform::BoardEntry for ThreadxQemuRiscv64 {
    fn run<F, E>(setup: F) -> Result<(), E>
    where
        F: FnOnce(&mut nros_platform::RuntimeCtx<'_>) -> Result<(), E>,
        E: core::fmt::Debug,
    {
        let cfg = Config::default();
        nros_board_threadx::run_entry::<ThreadxQemuRiscv64, Config, F, E>(cfg, None, setup)
    }

    /// Phase 245 B0 / issue #48 — apply the `nros::main!()` deploy overlay
    /// (`[package.metadata.nros.deploy.threadx-qemu-riscv64]`: locator / ip /
    /// gateway / netmask / domain_id) onto `Config::default()` before boot, so the
    /// Entry pkg's deploy metadata stops being inert. Fields the deploy block omits
    /// keep the board default.
    ///
    /// Issue #98 / RFC-0045 — also threads `deploy.boot_config` so the node name
    /// comes from the baked `.nros_boot_config`.
    fn run_with_deploy<F, E>(deploy: &nros_platform::DeployOverlay, setup: F) -> Result<(), E>
    where
        F: FnOnce(&mut nros_platform::RuntimeCtx<'_>) -> Result<(), E>,
        E: core::fmt::Debug,
    {
        nros_board_threadx::run_entry::<ThreadxQemuRiscv64, Config, F, E>(
            config_with_overlay(deploy),
            deploy.boot_config,
            setup,
        )
    }
}

/// Phase 245 — bare-metal CycloneDDS app-thread entry.
///
/// The CMake/CycloneDDS firmware boots through a **C** `startup.c::main` that
/// calls `tx_kernel_enter()` itself and dispatches to the example's Rust
/// `app_main` *inside* the spawned ThreadX app thread — so the kernel is already
/// running when `app_main` is reached. `app_main` must NOT call
/// [`nros_platform::BoardEntry::run`] (that re-enters the kernel); it calls this,
/// which runs the post-kernel body (open executor + `setup` + spin) on
/// `Config::default()`. The cargo/zenoh path uses `nros::main!()` /
/// `BoardEntry::run` instead and never reaches here.
///
/// CycloneDDS-path note: no `nros::main!()` macro emits a baked boot config for
/// this path, so `boot_config = None` (keeps the `"nros_app"` default).
pub fn run_app_thread<F, E>(setup: F) -> !
where
    F: FnOnce(&mut nros_platform::RuntimeCtx<'_>) -> Result<(), E>,
    E: core::fmt::Debug,
{
    nros_board_threadx::run_app_thread::<ThreadxQemuRiscv64, Config, F, E>(
        Config::default(),
        None,
        setup,
    )
}

/// Phase 245 B0 — overlay the `nros::main!()` deploy block onto `Config::default()`.
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

/// #131 — bare-metal `__assert_func` override.
///
/// newlib's `assert()` expands to `__assert_func`, whose default implementation
/// does `fprintf(stderr, …)` + `abort()`. On this bare-metal RISC-V image there
/// is no `stderr`, so pulling newlib's version (as registering the zenoh backend
/// does — zenoh-pico/zpico C code contains `assert()` calls) fails the link with
/// `undefined symbol: stderr`. Providing this strong definition satisfies every
/// `__assert_func` reference from the board's own object, so the linker never
/// pulls the stderr-dependent archive member. Routes to the UART + QEMU failure
/// exit instead, matching the panic handler.
///
/// # Safety
/// Called by C `assert()`; the four pointers are newlib-supplied NUL-terminated
/// C strings (`file`, `func`, `failedexpr`) or null. We only read them to print.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __assert_func(
    file: *const core::ffi::c_char,
    line: core::ffi::c_int,
    func: *const core::ffi::c_char,
    failedexpr: *const core::ffi::c_char,
) -> ! {
    unsafe fn puts_cstr(p: *const core::ffi::c_char) {
        if p.is_null() {
            return;
        }
        // Bounded walk so a corrupt/unterminated pointer can't loop forever.
        let mut i = 0usize;
        let mut buf = [0u8; 256];
        while i < buf.len() - 1 {
            let b = unsafe { *p.add(i) } as u8;
            if b == 0 {
                break;
            }
            buf[i] = b;
            i += 1;
        }
        if let Ok(s) = core::str::from_utf8(&buf[..i]) {
            uart_write_str(s);
        }
    }
    uart_write_str("ASSERT: ");
    unsafe { puts_cstr(failedexpr) };
    uart_write_str(" at ");
    unsafe { puts_cstr(file) };
    uart_write_str(":");
    {
        use core::fmt::Write;
        let mut buf = UartWriter;
        let _ = write!(buf, "{}", line);
    }
    uart_write_str(" (");
    unsafe { puts_cstr(func) };
    uart_write_str(")\n");
    exit_failure()
}
