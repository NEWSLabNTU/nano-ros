//! Platform initialization and `run()` entry point for QEMU FreeRTOS.
//!
//! Sequence:
//! 1. Init LAN9118 + lwIP via C glue (`nros_freertos_init_network`)
//! 2. Create a FreeRTOS application task that runs the user closure
//! 3. Create a network polling task
//! 4. Start the FreeRTOS scheduler (does not return)

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
}

/// Application task stack size in words (8 KB = 2048 words).
const APP_TASK_STACK: u32 = 2048;

/// Network polling task stack size in words (1 KB = 256 words).
const POLL_TASK_STACK: u32 = 256;

/// Application task priority (above normal, below tcpip_thread).
const APP_TASK_PRIORITY: u32 = 3;

/// Network poll task priority (low — just feeding frames to lwIP).
const POLL_TASK_PRIORITY: u32 = 1;

/// Network poll interval in milliseconds.
const POLL_INTERVAL_MS: u32 = 5;

/// Wrapper passed through the FreeRTOS task `void *` argument.
struct AppContext<F> {
    config: Config,
    closure: F,
}

/// FreeRTOS task entry for the application closure.
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
    // Bring in the FreeRTOS vTaskDelay via a raw symbol.  The tick rate is
    // 1 kHz (configTICK_RATE_HZ = 1000), so ticks ≈ milliseconds.
    unsafe extern "C" {
        fn vTaskDelay(ticks: u32);
    }

    loop {
        unsafe {
            nros_freertos_poll_network();
            vTaskDelay(POLL_INTERVAL_MS);
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
///     let mut executor = Executor::<_, 0, 0>::open(&exec_config)?;
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

    // Initialize LAN9118 + lwIP
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

    if let Err(e) = init_network(&config) {
        hprintln!("Error initializing network: {:?}", e);
        exit_failure();
    }
    hprintln!("Network ready.");
    hprintln!("");

    // Build the application context (lives on this stack frame, which persists
    // because `nros_freertos_start_scheduler()` never returns).
    let mut ctx = AppContext {
        config,
        closure: f,
    };

    // Create network poll task
    let ret = unsafe {
        nros_freertos_create_task(
            poll_task_entry,
            b"net_poll\0".as_ptr(),
            POLL_TASK_STACK,
            core::ptr::null_mut(),
            POLL_TASK_PRIORITY,
        )
    };
    if ret != 0 {
        hprintln!("Error creating network poll task");
        exit_failure();
    }

    // Create application task
    let ret = unsafe {
        nros_freertos_create_task(
            app_task_entry::<F, E>,
            b"nros_app\0".as_ptr(),
            APP_TASK_STACK,
            &mut ctx as *mut AppContext<F> as *mut c_void,
            APP_TASK_PRIORITY,
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
