//! Phase 152.1.B.5 — generic FreeRTOS `run<B>` lift.
//!
//! Lifted from `nros-board-mps2-an385-freertos`'s former
//! `node.rs`. The board-specific divergence (semihosting print,
//! QEMU semihosting exit, hardware init) is captured by the three
//! traits in `nros_board_common::board_init`:
//!
//! - [`BoardInit`] — `init_hardware(&Config)`
//! - [`BoardPrint`] — `println(format_args!(...))`
//! - [`BoardExit`] — `exit_success / exit_failure`
//!
//! Per-board overlays implement those traits + call
//! `nros_board_freertos::run::<MyBoard, _, _>(config, f)` from
//! their own `pub fn run` wrapper.

use core::ffi::c_void;

use nros_board_common::{BoardExit, BoardInit, BoardPrint};

use crate::{
    Config,
    error::{Error, Result},
};

unsafe extern "C" {
    fn nros_trace_scheduler_started();
    fn nros_trace_trigger_and_dump();

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
const POLL_TASK_STACK: u32 = 256;

struct AppContext<F> {
    config: Config,
    closure: F,
}

static mut POLL_INTERVAL_MS: u32 = 5;

/// FreeRTOS task entry for the application closure.
///
/// # Safety
/// `arg` must point to a valid `AppContext<F>` allocated on the
/// FreeRTOS heap by `run()`, surviving until the scheduler exits.
unsafe extern "C" fn app_task_entry<B, F, E>(arg: *mut c_void)
where
    B: BoardPrint + BoardExit,
    F: FnOnce(&Config) -> core::result::Result<(), E>,
    E: core::fmt::Debug,
{
    let ctx = unsafe { &mut *(arg as *mut AppContext<F>) };

    if let Err(e) = init_network(&ctx.config) {
        B::println(format_args!("Error initializing network: {:?}", e));
        B::exit_failure();
    }
    B::println(format_args!("Network ready."));
    B::println(format_args!(""));

    // Seed the platform RNG. Without this, both listener and
    // talker get identical xorshift output, causing duplicate
    // zenoh session IDs and connection rejection.
    {
        let ip = &ctx.config.ip;
        let mac = &ctx.config.mac;
        let mut seed = ((ip[0] as u32) << 24)
            | ((ip[1] as u32) << 16)
            | ((ip[2] as u32) << 8)
            | (ip[3] as u32);
        seed = seed.wrapping_mul(2654435761);
        seed ^= ((mac[4] as u32) << 8) | (mac[5] as u32);
        if seed == 0 {
            seed = 1;
        }
        unsafe extern "C" {
            fn nros_platform_freertos_seed_rng(value: u32);
        }
        unsafe { nros_platform_freertos_seed_rng(seed) };
    }

    let poll_pri = Config::to_freertos_priority(ctx.config.poll_priority);

    #[cfg(feature = "rmw-zenoh")]
    {
        let read_pri = Config::to_freertos_priority(ctx.config.zenoh_read_priority);
        let lease_pri = Config::to_freertos_priority(ctx.config.zenoh_lease_priority);
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

        // Phase 248 C5a (#60 T4) — board-owned RMW selection. Register the
        // linked zenoh backend before the user closure opens an executor.
        // FreeRTOS is `target_os = "none"` (linkme no-op, no `.init_array`), so
        // this explicit idempotent call replaces the prior reliance on
        // `nros::__register_linked_rmw()` via `nros/rmw-zenoh`.
        if let Err(err) = ::nros_rmw_zenoh::register() {
            B::println(format_args!(
                "nros: zenoh RMW backend register failed: {:?}",
                err
            ));
        }
    }

    unsafe {
        nros_trace_scheduler_started();
    }

    unsafe {
        POLL_INTERVAL_MS = ctx.config.poll_interval_ms;
    }
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
        B::println(format_args!("Error creating network poll task"));
        B::exit_failure();
    }

    // Brief delay so the poll task can flush stale RX and TAP +
    // bridge come up before TCP connections begin.
    unsafe {
        unsafe extern "C" {
            fn vTaskDelay(ticks: u32);
        }
        vTaskDelay(2000);
    }

    let netif_state = unsafe { nros_freertos_get_netif_state() };
    if netif_state & 0xF != 0xF {
        B::println(format_args!(
            "WARNING: lwIP netif not ready (default={} up={} link={} ip={})",
            netif_state & 1 != 0,
            netif_state & 2 != 0,
            netif_state & 4 != 0,
            netif_state & 8 != 0,
        ));
    }

    // FnOnce — `core::ptr::read` because this task entry is only
    // called once by FreeRTOS.
    let closure = unsafe { core::ptr::read(&ctx.closure) };

    match closure(&ctx.config) {
        Ok(()) => {
            unsafe {
                nros_trace_trigger_and_dump();
            }
            B::println(format_args!(""));
            B::println(format_args!("Application completed successfully."));
            B::println(format_args!(""));
            B::println(format_args!("========================================"));
            B::println(format_args!("  Done"));
            B::println(format_args!("========================================"));
            B::exit_success();
        }
        Err(e) => {
            unsafe {
                nros_trace_trigger_and_dump();
            }
            B::println(format_args!(""));
            B::println(format_args!("Application error: {:?}", e));
            B::exit_failure();
        }
    }
}

unsafe extern "C" fn poll_task_entry(_arg: *mut c_void) {
    unsafe extern "C" {
        fn vTaskDelay(ticks: u32);
    }
    let interval = unsafe { POLL_INTERVAL_MS };
    loop {
        unsafe {
            nros_freertos_poll_network();
            vTaskDelay(interval);
        }
    }
}

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

/// Generic FreeRTOS `run<B>` entry point.
///
/// Per-board overlays wrap this with a non-generic `pub fn run`
/// so user code stays free of trait turbofish.
///
/// # Type parameters
///
/// - `B: BoardInit<Config = Config> + BoardPrint + BoardExit` —
///   per-board glue (hardware init, print, exit).
/// - `F: FnOnce(&Config) -> Result<(), E>` — user closure.
/// - `E: core::fmt::Debug` — error type the closure returns.
pub fn run<B, F, E>(config: Config, f: F) -> !
where
    B: BoardInit<Config = Config> + BoardPrint + BoardExit,
    F: FnOnce(&Config) -> core::result::Result<(), E>,
    E: core::fmt::Debug,
{
    B::println(format_args!(""));
    B::println(format_args!("========================================"));
    B::println(format_args!("  nros FreeRTOS Platform"));
    B::println(format_args!("========================================"));
    B::println(format_args!(""));

    B::println(format_args!("Initializing LAN9118 + lwIP..."));
    B::println(format_args!(
        "  MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        config.mac[0], config.mac[1], config.mac[2], config.mac[3], config.mac[4], config.mac[5],
    ));
    B::println(format_args!(
        "  IP:  {}.{}.{}.{}",
        config.ip[0], config.ip[1], config.ip[2], config.ip[3],
    ));

    // Per-board pre-scheduler init (board crates may no-op).
    B::init_hardware(&config);

    let app_pri = Config::to_freertos_priority(config.app_priority);
    let app_stack_words = config.app_stack_bytes / 4;

    // Heap-allocate the app context: pre-scheduler MSP stack is
    // reclaimed by FreeRTOS when vPortStartFirstTask() resets MSP
    // to _estack. Local variables would be clobbered by the next
    // exception that stacks on MSP.
    // Task-context heap via the canonical platform ABI (RFC-0034 / phase-230
    // 1e). `nros_platform_alloc` wraps `pvPortMalloc` (heap_4) — same heap.
    unsafe extern "C" {
        fn nros_platform_alloc(size: usize) -> *mut c_void;
    }
    let ctx_ptr = unsafe {
        let size = core::mem::size_of::<AppContext<F>>();
        let ptr = nros_platform_alloc(size) as *mut AppContext<F>;
        assert!(!ptr.is_null(), "Failed to allocate AppContext");
        core::ptr::write(ptr, AppContext { config, closure: f });
        ptr
    };

    let ret = unsafe {
        nros_freertos_create_task(
            app_task_entry::<B, F, E>,
            b"nros_app\0".as_ptr(),
            app_stack_words,
            ctx_ptr as *mut c_void,
            app_pri,
        )
    };
    if ret != 0 {
        B::println(format_args!("Error creating application task"));
        B::exit_failure();
    }

    unsafe {
        nros_freertos_start_scheduler();
    }

    // Unreachable — scheduler never returns. Satisfies `-> !`.
    B::exit_failure()
}
