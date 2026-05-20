//! Phase 152.2.B.4 — generic `run<B>` for ThreadX overlays.
//!
//! Per-board overlays implement
//! `BoardInit + BoardPrint + BoardExit` + `ThreadxConfig` on
//! their `Config` and provide the `nros_threadx_set_config` /
//! `nros_threadx_set_app_callback` C symbols (via their
//! `c/board_threadx_<plat>.c` glue). This function:
//!
//! 1. Prints banner + IP/MAC via `B::println`.
//! 2. Calls per-board pre-kernel hardware init via
//!    `B::init_hardware(&cfg)`.
//! 3. Writes the closure + config into a fixed `static mut`
//!    storage block (size-asserted at runtime).
//! 4. Pushes config + callback through the unified
//!    `nros_threadx_set_config(...)` (5-arg) +
//!    `nros_threadx_set_app_callback(...)` FFI.
//! 5. Enters the ThreadX kernel via `tx_kernel_enter()` — never
//!    returns.
//! 6. Inside the spawned ThreadX thread, sleeps 200 ticks
//!    (2 s) for network stabilisation, then invokes the user
//!    closure. Exits via `B::exit_success/_failure` on
//!    Ok/Err.

use core::ffi::c_void;

use nros_board_common::{Board, BoardExit, BoardPrint, ThreadxConfig};

unsafe extern "C" {
    fn nros_threadx_set_config(
        ip: *const u8,
        netmask: *const u8,
        gateway: *const u8,
        mac: *const u8,
        interface_name: *const u8,
    );

    fn nros_threadx_set_app_callback(entry: unsafe extern "C" fn(*mut c_void), arg: *mut c_void);

    #[link_name = "_tx_initialize_kernel_enter"]
    fn tx_kernel_enter();

    #[link_name = "_tx_thread_sleep"]
    fn tx_thread_sleep(ticks: u32);
}

/// Wrapper passed through the ThreadX thread `void *` arg.
struct AppContext<C, F> {
    config: C,
    closure: F,
}

/// Static storage for the app context — sized for typical
/// closure captures (Executor handle + a handful of node
/// handles). Asserted at runtime in `run()` so overflow is
/// caught loudly instead of corrupting adjacent memory.
const CTX_STORAGE_SIZE: usize = 8192;
static mut CTX_STORAGE: [u8; CTX_STORAGE_SIZE] = [0u8; CTX_STORAGE_SIZE];

/// Static interface-name buffer for the C FFI's
/// `interface_name` argument. Linux overlay copies its
/// `Config::interface` here + appends NUL; bare-metal
/// overlays leave it empty and pass `NULL`.
const IFACE_BUF_SIZE: usize = 64;
static mut IFACE_BUF: [u8; IFACE_BUF_SIZE] = [0u8; IFACE_BUF_SIZE];

unsafe extern "C" fn app_task_entry<B, C, F, E>(arg: *mut c_void)
where
    B: BoardPrint + BoardExit,
    C: ThreadxConfig,
    F: FnOnce(&C) -> Result<(), E>,
    E: core::fmt::Debug,
{
    let ctx = unsafe { &*(arg as *const AppContext<C, F>) };

    // Network stabilisation delay. Ticks at TX_TIMER_TICKS_PER_SECOND
    // (100 by default) — 200 ticks ≈ 2 s, matching the historical
    // per-overlay wait.
    unsafe {
        tx_thread_sleep(200);
    }

    // FnOnce — `ptr::read` because this task entry runs once.
    let closure = unsafe { core::ptr::read(&ctx.closure) };

    match closure(&ctx.config) {
        Ok(()) => {
            B::println(format_args!(""));
            B::println(format_args!("Application completed successfully."));
            B::println(format_args!(""));
            B::println(format_args!("========================================"));
            B::println(format_args!("  Done"));
            B::println(format_args!("========================================"));
            B::exit_success();
        }
        Err(e) => {
            B::println(format_args!(""));
            B::println(format_args!("Application error: {:?}", e));
            B::exit_failure();
        }
    }
}

/// Generic ThreadX `run<B>` entry point.
///
/// Per-board overlays wrap this with a non-generic `pub fn run`
/// so user code stays free of trait turbofish.
///
/// # Type parameters
///
/// - `B: Board` — per-board glue (hardware init, print, exit), the
///   Phase 173.1 super-trait. `B::Config` is the board's config type;
///   the former standalone `C` param collapsed onto it (173.1).
/// - `B::Config: ThreadxConfig` — exposes
///   `mac/ip/netmask/gateway/interface()` accessors.
/// - `F: FnOnce(&B::Config) -> Result<(), E>` — user closure.
/// - `E: core::fmt::Debug` — error type the closure returns.
pub fn run<B, F, E>(config: B::Config, f: F) -> !
where
    B: Board,
    B::Config: ThreadxConfig,
    F: FnOnce(&B::Config) -> Result<(), E>,
    E: core::fmt::Debug,
{
    B::println(format_args!(""));
    B::println(format_args!("========================================"));
    B::println(format_args!("  nros ThreadX Platform"));
    B::println(format_args!("========================================"));
    B::println(format_args!(""));

    let mac = config.mac();
    let ip = config.ip();
    B::println(format_args!("Initializing ThreadX + NetX..."));
    B::println(format_args!(
        "  MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
    ));
    B::println(format_args!(
        "  IP:  {}.{}.{}.{}",
        ip[0], ip[1], ip[2], ip[3]
    ));
    if let Some(iface) = config.interface() {
        B::println(format_args!("  IF:  {}", iface));
    }

    B::init_hardware(&config);

    // Static-storage placement of AppContext. Closure size is
    // bounded by CTX_STORAGE_SIZE (8 KB) — asserted so overflow
    // is caught loudly instead of corrupting adjacent memory.
    let ctx_ptr = unsafe {
        let size = core::mem::size_of::<AppContext<B::Config, F>>();
        let align = core::mem::align_of::<AppContext<B::Config, F>>();
        assert!(
            size <= CTX_STORAGE_SIZE,
            "AppContext too large for CTX_STORAGE — bump CTX_STORAGE_SIZE"
        );
        let storage_ptr = core::ptr::addr_of_mut!(CTX_STORAGE) as *mut u8;
        let addr = storage_ptr as usize;
        let aligned = (addr + align - 1) & !(align - 1);
        let offset = aligned - addr;
        assert!(
            offset + size <= CTX_STORAGE_SIZE,
            "AppContext alignment + size exceeds CTX_STORAGE"
        );
        let ptr = storage_ptr.add(offset) as *mut AppContext<B::Config, F>;
        core::ptr::write(ptr, AppContext { config, closure: f });
        ptr
    };

    // Materialise interface name into the static buffer (with
    // NUL terminator) or pass NULL for bare-metal overlays.
    let iface_ptr: *const u8 = unsafe {
        let cfg = &(*ctx_ptr).config;
        match cfg.interface() {
            Some(iface) => {
                let buf_ptr = core::ptr::addr_of_mut!(IFACE_BUF) as *mut u8;
                let bytes = iface.as_bytes();
                let n = bytes.len().min(IFACE_BUF_SIZE - 1);
                core::ptr::copy_nonoverlapping(bytes.as_ptr(), buf_ptr, n);
                *buf_ptr.add(n) = 0;
                buf_ptr as *const u8
            }
            None => core::ptr::null(),
        }
    };

    unsafe {
        let cfg = &(*ctx_ptr).config;
        nros_threadx_set_config(
            cfg.ip().as_ptr(),
            cfg.netmask().as_ptr(),
            cfg.gateway().as_ptr(),
            cfg.mac().as_ptr(),
            iface_ptr,
        );
        nros_threadx_set_app_callback(app_task_entry::<B, B::Config, F, E>, ctx_ptr as *mut c_void);

        // Enter the ThreadX kernel — does not return.
        tx_kernel_enter();
    }

    B::exit_failure()
}
