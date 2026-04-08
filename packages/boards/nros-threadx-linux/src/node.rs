//! Platform initialization and `run()` entry point for ThreadX Linux simulation.
//!
//! Sequence:
//! 1. Print banner and network config
//! 2. Pass config + closure to C via FFI
//! 3. Call `tx_kernel_enter()` — never returns
//! 4. ThreadX calls `tx_application_define()` → creates IP stack → spawns app thread
//! 5. Inside app thread: brief sleep, then invoke the Rust closure

use std::ffi::c_void;

use crate::config::Config;

// FFI bindings to app_define.c and ThreadX
unsafe extern "C" {
    fn nros_threadx_set_config(
        ip: *const u8,
        netmask: *const u8,
        gateway: *const u8,
        mac: *const u8,
        interface_name: *const u8,
    );

    fn nros_threadx_set_app_callback(entry: unsafe extern "C" fn(*mut c_void), arg: *mut c_void);

    // ThreadX API names (tx_*) are C macros that expand to _tx_* symbols.
    // Rust doesn't see C macros, so we link directly to the actual symbols.
    #[link_name = "_tx_initialize_kernel_enter"]
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
/// `arg` must point to a valid `Box<AppContext<F>>` that was leaked
/// in `run()`.
unsafe extern "C" fn app_task_entry<F, E>(arg: *mut c_void)
where
    F: FnOnce(&Config) -> Result<(), E>,
    E: std::fmt::Debug,
{
    let ctx = unsafe { Box::from_raw(arg as *mut AppContext<F>) };

    // Brief delay for network stabilization. ThreadX timer ticks at
    // TX_TIMER_TICKS_PER_SECOND (100), so 200 ticks = 2 seconds.
    unsafe {
        unsafe extern "C" {
            #[link_name = "_tx_thread_sleep"]
            fn tx_thread_sleep(ticks: u32);
        }
        tx_thread_sleep(200);
    }

    match (ctx.closure)(&ctx.config) {
        Ok(()) => {
            println!();
            println!("Application completed successfully.");
            println!();
            println!("========================================");
            println!("  Done");
            println!("========================================");
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!();
            eprintln!("Application error: {:?}", e);
            std::process::exit(1);
        }
    }
}

/// Initialize pre-kernel hardware for ThreadX Linux simulation.
///
/// On ThreadX, hardware and network initialization (NetX Duo IP stack)
/// happens inside `tx_application_define()` in C code, after the kernel
/// starts. This function only performs minimal pre-kernel setup (currently
/// a no-op).
///
/// Provided for API consistency with other board crates. For full hardware
/// init, use [`run()`] which handles kernel startup and network init.
pub fn init_hardware(_config: &Config) {
    // ThreadX network init (NetX Duo) happens in tx_application_define() C code.
    // Nothing to do before the kernel starts.
}

/// Run an application on Linux with ThreadX + NetX Duo.
///
/// This is the main entry point for ThreadX Linux simulation applications.
/// It initializes the ThreadX kernel, NetX Duo IP stack (with Linux TAP
/// network driver), and calls the user closure inside a ThreadX thread.
///
/// Inside the closure, use `Executor::open()` to create an executor
/// with full API access (publishers, subscriptions, services, actions,
/// timers, callbacks).
///
/// # Example
///
/// ```ignore
/// use nros_threadx_linux::{Config, run};
/// use nros::prelude::*;
///
/// run(Config::default(), |config| {
///     let exec_config = ExecutorConfig::new(config.zenoh_locator)
///         .domain_id(config.domain_id);
///     let mut executor = Executor::open(&exec_config)?;
///     let mut node = executor.create_node("my_node")?;
///     // ...
///     Ok::<(), NodeError>(())
/// })
/// ```
///
/// # Returns
///
/// Never returns (`-> !`). Calls `std::process::exit(0)` on Ok,
/// `std::process::exit(1)` on Err.
pub fn run<F, E: std::fmt::Debug>(config: Config, f: F) -> !
where
    F: FnOnce(&Config) -> Result<(), E>,
{
    // Install SIGSEGV handler to print backtrace on crash
    unsafe {
        unsafe extern "C" {
            fn signal(sig: i32, handler: unsafe extern "C" fn(i32)) -> usize;
            fn backtrace(buffer: *mut *mut std::ffi::c_void, size: i32) -> i32;
            fn backtrace_symbols_fd(buffer: *const *mut std::ffi::c_void, size: i32, fd: i32);
            fn _exit(status: i32) -> !;
        }
        unsafe extern "C" fn segv_handler(_sig: i32) {
            let mut buf = [std::ptr::null_mut::<std::ffi::c_void>(); 64];
            let n = backtrace(buf.as_mut_ptr(), 64);
            unsafe extern "C" { fn write(fd: i32, buf: *const u8, count: usize) -> isize; }
            let msg = b"\n=== SIGSEGV backtrace ===\n";
            write(2, msg.as_ptr(), msg.len());
            backtrace_symbols_fd(buf.as_ptr(), n, 2);
            _exit(139);
        }
        signal(11, segv_handler); // SIGSEGV = 11
    }

    println!();
    println!("========================================");
    println!("  nros ThreadX Linux Platform");
    println!("========================================");
    println!();

    println!("Initializing ThreadX + NetX Duo...");
    println!(
        "  MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        config.mac[0], config.mac[1], config.mac[2], config.mac[3], config.mac[4], config.mac[5]
    );
    println!(
        "  IP:  {}.{}.{}.{}",
        config.ip[0], config.ip[1], config.ip[2], config.ip[3]
    );
    println!("  IF:  {}", config.interface);

    // Leak the context so it lives past tx_kernel_enter() (which never returns).
    let ctx = Box::new(AppContext {
        config: config.clone(),
        closure: f,
    });
    let ctx_ptr = Box::into_raw(ctx) as *mut c_void;

    // NUL-terminated interface name for C
    // Leak is intentional — tx_kernel_enter() never returns
    let iface_cstr = format!("{}\0", config.interface);
    let iface_ptr = iface_cstr.as_ptr();
    std::mem::forget(iface_cstr);

    unsafe {
        nros_threadx_set_config(
            config.ip.as_ptr(),
            config.netmask.as_ptr(),
            config.gateway.as_ptr(),
            config.mac.as_ptr(),
            iface_ptr,
        );

        nros_threadx_set_app_callback(app_task_entry::<F, E>, ctx_ptr);

        // Enter the ThreadX kernel — does not return
        tx_kernel_enter();
    }

    // Unreachable, but satisfies the `-> !` return type
    std::process::exit(1)
}
