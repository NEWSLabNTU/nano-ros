//! Platform initialization and `run()` entry point for QEMU FreeRTOS.
//!
//! Sequence:
//! 1. Print banner and network config
//! 2. Create a FreeRTOS application task
//! 3. Start the FreeRTOS scheduler (does not return)
//! 4. Inside the app task: init LAN9118 + lwIP, start poll task, run user closure

use core::ffi::c_void;

use cortex_m_semihosting::hprintln;

use crate::config::Config;
use crate::error::{Error, Result};
use crate::{exit_failure, exit_success};

// FFI bindings to the C startup/glue code compiled by build.rs
unsafe extern "C" {
    fn nros_freertos_init_network(
        mac: *const u8,
        ip: *const u8,
        netmask: *const u8,
        gw: *const u8,
    ) -> i32;

    fn nros_freertos_poll_network();
    fn nros_freertos_start_scheduler();

    fn nros_freertos_create_task(
        entry: unsafe extern "C" fn(*mut c_void),
        name: *const u8,
        stack_words: u32,
        arg: *mut c_void,
        priority: u32,
    ) -> i32;

    fn nros_freertos_get_netif_state() -> i32;
}

/// Network polling task stack size in words (1 KB = 256 words).
/// Not configurable — the poll task does minimal work (single function call).
const POLL_TASK_STACK: u32 = 256;

/// Wrapper passed through the FreeRTOS task `void *` argument.
struct AppContext<F> {
    config: Config,
    closure: F,
}

/// Poll interval shared with the poll task via a static.
/// Set by app_task_entry before creating the poll task.
static mut POLL_INTERVAL_MS: u32 = 5;

/// FreeRTOS task entry for the application closure.
///
/// Initializes the network stack (requires the scheduler to be running so
/// lwIP's tcpip_thread can execute), starts the poll task, then runs the
/// user closure.
///
/// # Safety
/// `arg` must point to a valid `AppContext<F>` allocated on the stack of
/// `run()` which lives until the scheduler exits (i.e., forever).
unsafe extern "C" fn app_task_entry<F, E>(arg: *mut c_void)
where
    F: FnOnce(&Config) -> core::result::Result<(), E>,
    E: core::fmt::Debug,
{
    let ctx = unsafe { &mut *(arg as *mut AppContext<F>) };

    // Initialize LAN9118 + lwIP. This must happen inside a task (after the
    // scheduler starts) because tcpip_init() creates the tcpip_thread, and
    // the init-done callback only fires once that thread runs. The busy-wait
    // with vTaskDelay() yields to it correctly now that the scheduler is active.
    if let Err(e) = init_network(&ctx.config) {
        hprintln!("Error initializing network: {:?}", e);
        exit_failure();
    }
    hprintln!("Network ready.");
    hprintln!("");

    // Configure zenoh-pico read/lease task scheduling from config.
    // Must be called before Executor::open() which calls zpico_open().
    let read_pri = Config::to_freertos_priority(ctx.config.zenoh_read_priority);
    let lease_pri = Config::to_freertos_priority(ctx.config.zenoh_lease_priority);
    let poll_pri = Config::to_freertos_priority(ctx.config.poll_priority);
    {
        unsafe extern "C" {
            fn zpico_set_task_config(
                read_priority: u32,
                read_stack_bytes: u32,
                lease_priority: u32,
                lease_stack_bytes: u32,
            );
        }
        unsafe {
            zpico_set_task_config(
                read_pri,
                ctx.config.zenoh_read_stack_bytes,
                lease_pri,
                ctx.config.zenoh_lease_stack_bytes,
            );
        }
    }

    // Start the network poll task AFTER init_network registers the netif.
    // Creating it earlier would poll an uninitialized netif during vTaskDelay
    // inside init_network.
    // Share poll interval with the poll task via static
    unsafe { POLL_INTERVAL_MS = ctx.config.poll_interval_ms; }
    let ret = unsafe {
        nros_freertos_create_task(
            poll_task_entry,
            b"net_poll\0".as_ptr(),
            POLL_TASK_STACK,
            core::ptr::null_mut(),
            poll_pri,
        )
    };
    if ret != 0 {
        hprintln!("Error creating network poll task");
        exit_failure();
    }

    // Brief delay to let the network stabilize: the poll task needs a few
    // iterations to flush stale RX data, and the TAP link + bridge need
    // time to come up before TCP connections can succeed.
    unsafe {
        unsafe extern "C" {
            fn vTaskDelay(ticks: u32);
        }
        vTaskDelay(2000); // 2 seconds at 1 kHz tick rate
    }

    // Verify lwIP netif state before running application
    let netif_state = unsafe { nros_freertos_get_netif_state() };
    if netif_state & 0xF != 0xF {
        hprintln!(
            "WARNING: lwIP netif not ready (default={} up={} link={} ip={})",
            netif_state & 1 != 0,
            netif_state & 2 != 0,
            netif_state & 4 != 0,
            netif_state & 8 != 0,
        );
    }

    // Take the closure out of the context so we can call it (FnOnce).
    // Safety: this task entry is only called once by FreeRTOS.
    let closure = unsafe { core::ptr::read(&ctx.closure) };

    match closure(&ctx.config) {
        Ok(()) => {
            hprintln!("");
            hprintln!("Application completed successfully.");
            hprintln!("");
            hprintln!("========================================");
            hprintln!("  Done");
            hprintln!("========================================");
            exit_success();
        }
        Err(e) => {
            hprintln!("");
            hprintln!("Application error: {:?}", e);
            exit_failure();
        }
    }
}

/// FreeRTOS task that polls the LAN9118 RX FIFO periodically.
unsafe extern "C" fn poll_task_entry(_arg: *mut c_void) {
    unsafe extern "C" {
        fn vTaskDelay(ticks: u32);
    }

    // Read the configured poll interval (set by app_task_entry before
    // this task is created). Safe: single-writer (app task writes once
    // before creating this task) then read-only.
    let interval = unsafe { POLL_INTERVAL_MS };

    loop {
        unsafe {
            nros_freertos_poll_network();
            vTaskDelay(interval);
        }
    }
}

/// Initialize the network stack.
fn init_network(config: &Config) -> Result<()> {
    let ret = unsafe {
        nros_freertos_init_network(
            config.mac.as_ptr(),
            config.ip.as_ptr(),
            config.netmask.as_ptr(),
            config.gateway.as_ptr(),
        )
    };
    if ret != 0 {
        return Err(Error::NetworkInit);
    }
    Ok(())
}

/// Initialize pre-scheduler hardware for MPS2-AN385 FreeRTOS.
///
/// On FreeRTOS, most hardware initialization (LAN9118 + lwIP) must happen
/// inside a FreeRTOS task after the scheduler starts. This function only
/// performs the minimal pre-scheduler setup (currently a no-op — all init
/// happens inside [`run()`]).
///
/// Provided for API consistency with other board crates. For full hardware
/// init, use [`run()`] which handles scheduler startup and network init.
pub fn init_hardware(_config: &Config) {
    // FreeRTOS network init requires the scheduler to be running (tcpip_init
    // creates tcpip_thread). All meaningful init happens inside the app task
    // created by run().
}

/// Run an application on QEMU MPS2-AN385 with FreeRTOS + lwIP.
///
/// This is the main entry point for FreeRTOS board applications.
/// It initialises hardware, starts the FreeRTOS scheduler, and calls
/// the user closure inside a FreeRTOS task.
///
/// Inside the closure, use `Executor::open()` to create an executor
/// with full API access (publishers, subscriptions, services, actions,
/// timers, callbacks).
///
/// # Example
///
/// ```ignore
/// use nros_mps2_an385_freertos::{Config, run};
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
/// Never returns (`-> !`). Calls `exit_success()` on Ok, `exit_failure()`
/// on Err.
pub fn run<F, E: core::fmt::Debug>(config: Config, f: F) -> !
where
    F: FnOnce(&Config) -> core::result::Result<(), E>,
{
    hprintln!("");
    hprintln!("========================================");
    hprintln!("  nros QEMU FreeRTOS Platform");
    hprintln!("========================================");
    hprintln!("");

    hprintln!("Initializing LAN9118 + lwIP...");
    hprintln!(
        "  MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        config.mac[0],
        config.mac[1],
        config.mac[2],
        config.mac[3],
        config.mac[4],
        config.mac[5]
    );
    hprintln!(
        "  IP:  {}.{}.{}.{}",
        config.ip[0],
        config.ip[1],
        config.ip[2],
        config.ip[3]
    );

    // Save scheduling values before config is moved into the heap context.
    let app_pri = Config::to_freertos_priority(config.app_priority);
    let app_stack_words = config.app_stack_bytes / 4;

    // Allocate the application context on the FreeRTOS heap.
    //
    // CRITICAL: The pre-scheduler MSP stack is reclaimed by FreeRTOS when
    // vPortStartFirstTask() resets MSP to _estack. After that, SysTick
    // and other exception handlers use the same memory for stacking,
    // corrupting any local variables. Using pvPortMalloc() places the
    // context in heap memory that is safe from MSP reuse.
    unsafe extern "C" {
        fn pvPortMalloc(size: u32) -> *mut c_void;
    }

    let ctx_ptr = unsafe {
        let size = core::mem::size_of::<AppContext<F>>() as u32;
        let ptr = pvPortMalloc(size) as *mut AppContext<F>;
        assert!(!ptr.is_null(), "Failed to allocate AppContext");
        core::ptr::write(
            ptr,
            AppContext {
                config,
                closure: f,
            },
        );
        ptr
    };

    // Create application task — network init and poll task creation happen
    // inside this task after the scheduler starts, because tcpip_init()
    // requires the scheduler to run its tcpip_thread.
    let ret = unsafe {
        nros_freertos_create_task(
            app_task_entry::<F, E>,
            b"nros_app\0".as_ptr(),
            app_stack_words,
            ctx_ptr as *mut c_void,
            app_pri,
        )
    };
    if ret != 0 {
        hprintln!("Error creating application task");
        exit_failure();
    }

    // Start FreeRTOS — does not return
    unsafe {
        nros_freertos_start_scheduler();
    }

    // Unreachable, but satisfy the `-> !` return type
    exit_failure()
}
