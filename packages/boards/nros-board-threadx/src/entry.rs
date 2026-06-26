//! Phase 212.N.2 — `BoardEntry::run` shim for the ThreadX family.
//!
//! Adds an additive entry point built on the 212.N.1 trait set in
//! `nros_platform::board` (`BoardInit` parameterless, `BoardPrint`,
//! `BoardExit`, `RuntimeCtx`). Mirrors the legacy [`crate::run`]
//! body — kernel-spawn shape: stash the user closure into static
//! storage, push the network config + app callback through the C
//! glue (`nros_threadx_set_config` + `nros_threadx_set_app_callback`),
//! call `tx_kernel_enter()`, never return — but threads the new
//! `RuntimeCtx` through the user setup callback instead of an opaque
//! `&Config`.
//!
//! ## Why a free fn (not a blanket `impl BoardEntry`)
//!
//! The new [`nros_platform::BoardEntry`] trait is
//!
//! ```ignore
//! fn run<F, E>(setup: F) -> Result<(), E>
//!     where F: FnOnce(&mut RuntimeCtx<'_>) -> Result<(), E>,
//!           E: core::fmt::Debug;
//! ```
//!
//! ThreadX bring-up needs a board `Config` (MAC / IP / netmask /
//! gateway / interface) — that lives outside `RuntimeCtx` (codegen
//! overlay knobs, not hardware config). The per-board crate
//! (`nros-board-threadx-linux`, `nros-board-threadx-qemu-riscv64`)
//! owns the `Config` source (TOML / `Config::default()`); it
//! implements `BoardEntry` directly and delegates here:
//!
//! ```ignore
//! impl BoardEntry for MyBoard {
//!     fn run<F, E>(setup: F) -> Result<(), E>
//!     where
//!         F: FnOnce(&mut RuntimeCtx<'_>) -> Result<(), E>,
//!         E: core::fmt::Debug,
//!     {
//!         let cfg = Config::default();
//!         nros_board_threadx::run_entry::<MyBoard, Config, F, E>(cfg, setup)
//!     }
//! }
//! ```
//!
//! 212.N.3 wires that into `nros-board-threadx-linux` +
//! `nros-board-threadx-qemu-riscv64`; this file just provides the
//! family-side helper. The legacy [`crate::run`] coexists during the
//! 212.N transition.

use core::ffi::c_void;

use nros_board_common::ThreadxConfig;
use nros_platform::{BakedBootConfig, BoardExit, BoardInit, BoardPrint, RuntimeCtx};

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
    /// Issue #98 / RFC-0045 — baked `.nros_boot_config` for node-name resolution.
    boot_config: Option<&'static BakedBootConfig>,
    closure: F,
}

/// Static storage for the 212.N.2 path's `AppContext`. Distinct from
/// the legacy `node::run`'s `CTX_STORAGE` so both entry points can
/// coexist during the 212.N migration; per-board overlays only ever
/// link one path at a time, but keeping the statics separate avoids a
/// type / generic-parameter clash if a future overlay accidentally
/// pulls both. Sized for typical closure captures (Executor handle +
/// a handful of node handles); asserted at runtime in `run_entry()`
/// so overflow is caught loudly instead of corrupting adjacent
/// memory.
// Phase 214.H.1 — both constants live in the crate-level `sizes` module
// (`lib.rs`); see there for the rationale + bump procedure.
use crate::sizes::{CTX_STORAGE_SIZE, IFACE_BUF_SIZE};

static mut CTX_STORAGE: [u8; CTX_STORAGE_SIZE] = [0u8; CTX_STORAGE_SIZE];

/// Static interface-name buffer for the C FFI's `interface_name`
/// argument. Linux overlay copies its `Config::interface` here +
/// appends NUL; bare-metal overlays leave it empty and pass `NULL`.
static mut IFACE_BUF: [u8; IFACE_BUF_SIZE] = [0u8; IFACE_BUF_SIZE];

/// ThreadX task entry for the application closure (212.N flavour —
/// hands the closure a `&mut RuntimeCtx<'_>` instead of `&Config`).
///
/// # Safety
/// `arg` must point to a valid `AppContext<C, F>` written into
/// `CTX_STORAGE` by `run_entry()`, surviving until the ThreadX
/// kernel terminates.
unsafe extern "C" fn app_task_entry_runtime<B, C, F, E>(arg: *mut c_void)
where
    B: BoardPrint + BoardExit,
    C: ThreadxConfig,
    F: FnOnce(&mut RuntimeCtx<'_>) -> core::result::Result<(), E>,
    E: core::fmt::Debug,
{
    let ctx = unsafe { &*(arg as *const AppContext<C, F>) };

    // FnOnce / by-value config — `core::ptr::read` because this task
    // entry runs once and `run_app_thread` consumes both.
    let closure = unsafe { core::ptr::read(&ctx.closure) };
    let config = unsafe { core::ptr::read(&ctx.config) };
    let boot_config = ctx.boot_config;

    run_app_thread::<B, C, F, E>(config, boot_config, closure)
}

/// Phase 245 — the post-kernel app-thread body, factored out of
/// [`app_task_entry_runtime`] so the **bare-metal CycloneDDS path** can reuse it.
///
/// The cargo/zenoh path enters via [`run_entry`] (which calls `tx_kernel_enter`
/// and registers [`app_task_entry_runtime`] as the app callback). The
/// CMake/CycloneDDS firmware instead has a **C** `startup.c::main` that calls
/// `tx_kernel_enter` itself and dispatches to a Rust `app_main` — so by the time
/// `app_main` runs, **the kernel is already entered**. `app_main` must therefore
/// NOT call [`run_entry`] (double kernel-enter); it calls this directly.
///
/// Body: network-stabilisation sleep, open the executor (locator/domain from the
/// board `ThreadxConfig` — CycloneDDS ignores the locator, no router), wrap it in
/// an `ExecutorNodeRuntime`, run the user `setup` (the `nros::node!()`-emitted
/// `register`), then spin for the firmware lifetime. Diverges (`-> !`): the
/// ThreadX scheduler never lets the app thread exit.
///
/// `boot_config` — the baked `.nros_boot_config` static (issue #98 / RFC-0045).
/// Pass `None` for the bare-metal CycloneDDS path (no macro, no baked config).
pub fn run_app_thread<B, C, F, E>(
    config: C,
    boot_config: Option<&'static BakedBootConfig>,
    setup: F,
) -> !
where
    B: BoardPrint + BoardExit,
    C: ThreadxConfig,
    F: FnOnce(&mut RuntimeCtx<'_>) -> core::result::Result<(), E>,
    E: core::fmt::Debug,
{
    // Network stabilisation delay. Ticks at TX_TIMER_TICKS_PER_SECOND
    // (100 by default) — 200 ticks ≈ 2 s, matching the legacy per-
    // overlay wait in `node::app_task_entry`.
    unsafe {
        tx_thread_sleep(200);
    }

    // Issue #98 / RFC-0045 — node name from the baked `.nros_boot_config`
    // (a launch that names the node overrides the board default); locator +
    // domain_id come from the per-board `ThreadxConfig` (NOT env vars —
    // embedded libc `getenv` has no host trampoline).
    let baked = boot_config
        .map(::nros::BootConfig::from_baked)
        .unwrap_or_default();
    let exec_cfg = ::nros::ExecutorConfig::resolve(
        ::nros::BootConfig {
            node_name: baked.node_name.or(Some("nros_app")),
            locator: Some(config.zenoh_locator()),
            domain_id: Some(config.domain_id()),
            namespace: None,
        },
        /* hosted_env = */ false,
    );
    let executor = match ::nros::Executor::open(&exec_cfg) {
        Ok(e) => e,
        Err(err) => {
            B::println(format_args!(""));
            B::println(format_args!("Executor::open failed: {:?}", err));
            B::exit_failure();
        }
    };
    let mut crt = ::nros::node_runtime::ExecutorNodeRuntime::from_executor(executor);
    let mut runtime = RuntimeCtx::with_runtime(&mut crt);

    match setup(&mut runtime) {
        Ok(()) => {
            B::println(format_args!(""));
            B::println(format_args!(
                "Application setup complete — entering spin loop."
            ));
            // Embedded spin: the ThreadX scheduler never returns from
            // this thread, so we loop forever. `spin_once` errors trip
            // exit_failure (a working bring-up never gets here).
            loop {
                if let Err(err) = ::nros_platform::NodeDispatchRuntime::spin_once(&mut crt, 10) {
                    B::println(format_args!(""));
                    B::println(format_args!("spin_once error: {:?}", err));
                    B::exit_failure();
                }
            }
        }
        Err(e) => {
            B::println(format_args!(""));
            B::println(format_args!("Application error: {:?}", e));
            B::exit_failure();
        }
    }
}

/// Phase 212.N.2 — family-driver entry point for ThreadX boards.
///
/// Mirrors the legacy [`crate::run`] body — stashes the user closure
/// into static storage, registers the network config + app callback
/// through the unified ThreadX C glue, calls `tx_kernel_enter()`,
/// never returns — but routes through the 212.N.1
/// `nros_platform::board` trait set + [`RuntimeCtx`].
///
/// Per-board crates (e.g. `nros-board-threadx-linux`,
/// `nros-board-threadx-qemu-riscv64`) wire this into their
/// `impl BoardEntry for Self::run` body in 212.N.3:
///
/// ```ignore
/// impl nros_platform::board::BoardEntry for MyBoard {
///     fn run<F, E>(setup: F) -> Result<(), E>
///     where
///         F: FnOnce(&mut RuntimeCtx<'_>) -> Result<(), E>,
///         E: core::fmt::Debug,
///     {
///         let cfg = Config::default();
///         nros_board_threadx::run_entry::<MyBoard, Config, F, E>(cfg, None, setup)
///     }
/// }
/// ```
///
/// # Type parameters
///
/// - `B: BoardInit + BoardPrint + BoardExit` — per-board glue pulled
///   from `nros_platform::board` (212.N.1 surface).
/// - `C: ThreadxConfig` — board's config type, exposing
///   `mac/ip/netmask/gateway/interface()` accessors. Stays as a
///   separate generic (rather than folded onto `B::Config`) so the
///   per-board overlay can keep its existing concrete `Config`
///   struct unchanged during the 212.N migration.
/// - `F: FnOnce(&mut RuntimeCtx<'_>) -> Result<(), E>` — user closure
///   receiving the runtime context.
/// - `E: core::fmt::Debug` — closure error type.
///
/// # Return
///
/// The signature is `Result<(), E>` to satisfy the
/// [`nros_platform::BoardEntry::run`] trait contract, but in practice
/// the kernel-spawn flow never returns to the caller — either
/// `tx_kernel_enter()` runs forever and the app thread drives
/// `exit_success` / `exit_failure`, or kernel entry itself returns
/// (e.g. on the Linux ThreadX-sim port after a clean shutdown) and we
/// `exit_failure` defensively. The `Ok(())` arm exists only so the
/// function signature lines up with the trait; it is unreachable in
/// a working build.
///
/// `boot_config` — the baked `.nros_boot_config` static, passed from
/// the per-board `run_with_deploy` (issue #98 / RFC-0045). `None`
/// keeps the historical `"nros_app"` node-name default.
pub fn run_entry<B, C, F, E>(
    config: C,
    boot_config: Option<&'static BakedBootConfig>,
    setup: F,
) -> core::result::Result<(), E>
where
    B: BoardInit + BoardPrint + BoardExit,
    C: ThreadxConfig,
    F: FnOnce(&mut RuntimeCtx<'_>) -> core::result::Result<(), E>,
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

    // Per-board pre-kernel init. New 212.N.1 `BoardInit::init_hardware`
    // is parameterless — board crates read any needed config off
    // their own `pub const` / `pub static` rather than a passed-in
    // arg.
    B::init_hardware();

    // Static-storage placement of AppContext. Closure size is
    // bounded by CTX_STORAGE_SIZE (8 KB) — asserted so overflow is
    // caught loudly instead of corrupting adjacent memory.
    let ctx_ptr = unsafe {
        let size = core::mem::size_of::<AppContext<C, F>>();
        let align = core::mem::align_of::<AppContext<C, F>>();
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
        let ptr = storage_ptr.add(offset) as *mut AppContext<C, F>;
        core::ptr::write(
            ptr,
            AppContext {
                config,
                boot_config,
                closure: setup,
            },
        );
        ptr
    };

    // Materialise interface name into the static buffer (with NUL
    // terminator) or pass NULL for bare-metal overlays.
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
        nros_threadx_set_app_callback(app_task_entry_runtime::<B, C, F, E>, ctx_ptr as *mut c_void);

        // Enter the ThreadX kernel — does not return on a working
        // bring-up.
        tx_kernel_enter();
    }

    // Unreachable — kernel enter diverges on production paths.
    // `exit_failure()` is `-> !`, so this satisfies the
    // `Result<(), E>` signature without an explicit `Ok` arm.
    B::exit_failure()
}
