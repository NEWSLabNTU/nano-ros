//! Platform initialization and `run()` entry point for ThreadX QEMU RISC-V.
//!
//! Sequence:
//! 1. Print banner and network config via UART
//! 2. Pass config + closure to C via FFI
//! 3. Call `tx_kernel_enter()` — never returns
//! 4. ThreadX calls `tx_application_define()` → creates IP stack → spawns app thread
//! 5. Inside app thread: brief sleep, then invoke the Rust closure

use core::ffi::c_void;

use crate::config::Config;
use crate::{exit_failure, exit_success, uart_write_str, UartWriter};

// FFI bindings to app_define.c and ThreadX
unsafe extern "C" {
    fn nros_threadx_set_config(
        ip: *const u8,
        netmask: *const u8,
        gateway: *const u8,
        mac: *const u8,
    );

    fn nros_threadx_set_app_callback(entry: unsafe extern "C" fn(*mut c_void), arg: *mut c_void);

    fn tx_kernel_enter();
}

/// Wrapper passed through the ThreadX thread `void *` argument.
struct AppContext<F> {
    config: Config,
    closure: F,
}

/// ThreadX app thread entry — invokes the Rust closure.
///
/// # Safety
/// `arg` must point to a valid `AppContext<F>`.
unsafe extern "C" fn app_task_entry<F, E>(arg: *mut c_void)
where
    F: FnOnce(&Config) -> core::result::Result<(), E>,
    E: core::fmt::Debug,
{
    let ctx = unsafe { &*(arg as *const AppContext<F>) };

    // Brief delay for network stabilization. ThreadX timer ticks at
    // TX_TIMER_TICKS_PER_SECOND (100), so 200 ticks = 2 seconds.
    unsafe {
        unsafe extern "C" {
            fn tx_thread_sleep(ticks: u32);
        }
        tx_thread_sleep(200);
    }

    // Take the closure out of the context so we can call it (FnOnce).
    let closure = unsafe { core::ptr::read(&ctx.closure) };

    match closure(&ctx.config) {
        Ok(()) => {
            uart_write_str("\n");
            uart_write_str("Application completed successfully.\n");
            uart_write_str("\n");
            uart_write_str("========================================\n");
            uart_write_str("  Done\n");
            uart_write_str("========================================\n");
            exit_success();
        }
        Err(e) => {
            uart_write_str("\nApplication error: ");
            {
                use core::fmt::Write;
                let mut buf = UartWriter;
                let _ = write!(buf, "{:?}", e);
            }
            uart_write_str("\n");
            exit_failure();
        }
    }
}

/// Run an application on QEMU RISC-V with ThreadX + NetX Duo + virtio-net.
///
/// This is the main entry point for ThreadX QEMU RISC-V applications.
/// It initializes the ThreadX kernel, NetX Duo IP stack (with virtio-net
/// driver), and calls the user closure inside a ThreadX thread.
///
/// Inside the closure, use `Executor::open()` to create an executor
/// with full API access (publishers, subscriptions, services, actions,
/// timers, callbacks).
///
/// # Example
///
/// ```ignore
/// use nros_threadx_qemu_riscv64::{Config, run};
/// use nros::prelude::*;
///
/// run(Config::default(), |config| {
///     let exec_config = ExecutorConfig::new(config.zenoh_locator)
///         .domain_id(config.domain_id);
///     let mut executor = Executor::<_, 0, 0>::open(&exec_config)?;
///     let mut node = executor.create_node("my_node")?;
///     // ...
///     Ok::<(), NodeError>(())
/// })
/// ```
///
/// # Returns
///
/// Never returns (`-> !`). Calls `exit_success()` on Ok,
/// `exit_failure()` on Err.
pub fn run<F, E: core::fmt::Debug>(config: Config, f: F) -> !
where
    F: FnOnce(&Config) -> core::result::Result<(), E>,
{
    use core::fmt::Write;
    let mut w = UartWriter;

    let _ = writeln!(w);
    let _ = writeln!(w, "========================================");
    let _ = writeln!(w, "  nros ThreadX QEMU RISC-V Platform");
    let _ = writeln!(w, "========================================");
    let _ = writeln!(w);

    let _ = writeln!(w, "Initializing ThreadX + NetX Duo + virtio-net...");
    let _ = writeln!(
        w,
        "  MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        config.mac[0], config.mac[1], config.mac[2], config.mac[3], config.mac[4], config.mac[5]
    );
    let _ = writeln!(
        w,
        "  IP:  {}.{}.{}.{}",
        config.ip[0], config.ip[1], config.ip[2], config.ip[3]
    );

    // Static context — lives until tx_kernel_enter() starts the app thread.
    // We use a static mut because there's no heap on bare metal before ThreadX starts.
    static mut CTX_STORAGE: [u8; 4096] = [0u8; 4096];

    let ctx_ptr = unsafe {
        let size = core::mem::size_of::<AppContext<F>>();
        assert!(size <= CTX_STORAGE.len(), "AppContext too large for static storage");
        let ptr = CTX_STORAGE.as_mut_ptr() as *mut AppContext<F>;
        core::ptr::write(
            ptr,
            AppContext {
                config,
                closure: f,
            },
        );
        ptr
    };

    unsafe {
        nros_threadx_set_config(
            (*ctx_ptr).config.ip.as_ptr(),
            (*ctx_ptr).config.netmask.as_ptr(),
            (*ctx_ptr).config.gateway.as_ptr(),
            (*ctx_ptr).config.mac.as_ptr(),
        );

        nros_threadx_set_app_callback(app_task_entry::<F, E>, ctx_ptr as *mut c_void);

        // Enter the ThreadX kernel — does not return
        tx_kernel_enter();
    }

    // Unreachable, but satisfies the `-> !` return type
    exit_failure()
}
