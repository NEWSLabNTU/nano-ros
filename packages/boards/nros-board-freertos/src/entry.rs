//! Phase 212.N.2 — `BoardEntry::run` shim for the FreeRTOS family.
//!
//! Adds an additive entry point built on the 212.N.1 trait set in
//! `nros_platform::board` (`BoardInit` parameterless, `BoardPrint`,
//! `BoardExit`, `RuntimeCtx`). Mirrors the legacy
//! [`crate::run`] body — kernel-spawn shape: allocate the app task,
//! hand it the user closure, call `vTaskStartScheduler()`, never
//! return — but threads the new `RuntimeCtx` through the user setup
//! callback instead of an opaque `&Config`.
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
//! FreeRTOS bring-up needs a board [`Config`] (MAC / IP / netmask /
//! gateway / task priorities + stack sizes) — that lives outside
//! `RuntimeCtx` (codegen overlay knobs, not hardware config). The
//! per-board crate (`nros-board-mps2-an385-freertos`, …) owns the
//! `Config` source (TOML / `Config::default()`); it implements
//! `BoardEntry` directly and delegates here:
//!
//! ```ignore
//! impl BoardEntry for MyBoard {
//!     fn run<F, E>(setup: F) -> Result<(), E>
//!     where
//!         F: FnOnce(&mut RuntimeCtx<'_>) -> Result<(), E>,
//!         E: core::fmt::Debug,
//!     {
//!         let cfg = Config::default();
//!         nros_board_freertos::run_entry::<MyBoard, F, E>(cfg, setup)
//!     }
//! }
//! ```
//!
//! 212.N.3 wires that into `nros-board-mps2-an385-freertos`; this
//! file just provides the family-side helper. The legacy
//! [`crate::run`] coexists during the 212.N transition.

use core::ffi::c_void;

use nros_platform::{BoardExit, BoardInit, BoardPrint, RuntimeCtx, TierSpec};

use crate::{
    Config,
    error::{Error, Result as FrResult},
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

/// FreeRTOS task entry for the application closure (212.N flavour —
/// hands the closure a `&mut RuntimeCtx<'_>` instead of `&Config`).
///
/// # Safety
/// `arg` must point to a valid `AppContext<F>` allocated on the
/// FreeRTOS heap by `run_entry()`, surviving until the scheduler
/// exits.
unsafe extern "C" fn app_task_entry_runtime<B, F, E>(arg: *mut c_void)
where
    B: BoardPrint + BoardExit,
    F: FnOnce(&mut RuntimeCtx<'_>) -> core::result::Result<(), E>,
    E: core::fmt::Debug,
{
    let ctx = unsafe { &mut *(arg as *mut AppContext<F>) };

    // Phase 228.E.2 — the boot bringup (network + RNG + poll task + zenoh task
    // config + netif wait) is shared with the per-tier app task.
    unsafe { freertos_boot_bringup::<B>(&ctx.config) };

    // FnOnce — `core::ptr::read` because this task entry is only
    // called once by FreeRTOS.
    let closure = unsafe { core::ptr::read(&ctx.closure) };

    // Phase 212.N.7 step-3.5 — open the executor + wrap it in an
    // `ExecutorNodeRuntime` so the codegen-emitted
    // `run_plan(runtime)` body can register components against a
    // live RMW session. Locator + domain_id come from `Config` (the
    // FreeRTOS overlay's TOML / default), NOT env vars — embedded
    // libc `getenv` has no host trampoline on QEMU. After the
    // closure returns Ok, the app task drops into a spin loop; the
    // scheduler never lets `main` return so the loop runs for the
    // firmware lifetime.
    let exec_cfg = ::nros::ExecutorConfig::new(ctx.config.zenoh_locator)
        .domain_id(ctx.config.domain_id)
        .node_name("nros_app");
    let executor = match ::nros::Executor::open(&exec_cfg) {
        Ok(e) => e,
        Err(err) => {
            unsafe {
                nros_trace_trigger_and_dump();
            }
            B::println(format_args!(""));
            B::println(format_args!("Executor::open failed: {:?}", err));
            B::exit_failure();
        }
    };
    let mut crt = ::nros::node_runtime::ExecutorNodeRuntime::from_executor(executor);
    let mut runtime = RuntimeCtx::with_runtime(&mut crt);

    match closure(&mut runtime) {
        Ok(()) => {
            B::println(format_args!(""));
            B::println(format_args!(
                "Application setup complete — entering spin loop."
            ));
            // Embedded spin: the FreeRTOS scheduler never returns from
            // this task, so we loop forever. `spin_once` errors trip
            // the trace dump + exit_failure (a working bring-up never
            // gets here).
            loop {
                if let Err(err) = ::nros_platform::NodeDispatchRuntime::spin_once(&mut crt, 10) {
                    unsafe {
                        nros_trace_trigger_and_dump();
                    }
                    B::println(format_args!(""));
                    B::println(format_args!("spin_once error: {:?}", err));
                    B::exit_failure();
                }
            }
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

// =============================================================================
// Phase 228.E.2 — per-tier multi-task entry (RFC-0032 §5, §8.2)
// =============================================================================

/// Shared boot bringup: network init + RNG seed + poll task + zenoh task config
/// + netif wait. Extracted from `app_task_entry_runtime` so the per-tier app
/// task reuses the exact same sequence.
///
/// # Safety
/// Runs inside a FreeRTOS task, pre-`Executor::open`. `config` must be valid.
unsafe fn freertos_boot_bringup<B>(config: &Config)
where
    B: BoardPrint + BoardExit,
{
    if let Err(e) = init_network(config) {
        B::println(format_args!("Error initializing network: {:?}", e));
        B::exit_failure();
    }
    B::println(format_args!("Network ready."));
    B::println(format_args!(""));

    // Seed the platform RNG so distinct sessions get distinct xorshift output.
    {
        let ip = &config.ip;
        let mac = &config.mac;
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

    let poll_pri = Config::to_freertos_priority(config.poll_priority);

    #[cfg(feature = "rmw-zenoh")]
    {
        let read_pri = Config::to_freertos_priority(config.zenoh_read_priority);
        let lease_pri = Config::to_freertos_priority(config.zenoh_lease_priority);
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
                config.zenoh_read_stack_bytes,
                lease_pri,
                config.zenoh_lease_stack_bytes,
            );
        }

        // Phase 248 C5a (#60 T4) — the board owns RMW selection. Register the
        // linked zenoh backend into the CFFI vtable here, before any
        // `Executor::open` (`app_task_entry_runtime` + `app_task_entry_tiers`
        // both bring up through this fn). FreeRTOS is `target_os = "none"`
        // (linkme is a no-op + the flat image runs no `.init_array`), so without
        // this explicit, idempotent call `resolve_backend` finds no transport and
        // `Executor::open` fails with `Transport(ConnectionFailed)`. Replaces the
        // prior reliance on `nros::__register_linked_rmw()` via `nros/rmw-zenoh`.
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
        POLL_INTERVAL_MS = config.poll_interval_ms;
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

    // Brief delay so the poll task flushes stale RX + the TAP settles.
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
}

// Task-context heap goes through the canonical platform ABI (RFC-0034 /
// phase-230 1e). On FreeRTOS boards `nros_platform_alloc` wraps `pvPortMalloc`
// (heap_4) — same heap, single funnel.
unsafe extern "C" {
    fn nros_platform_alloc(size: usize) -> *mut c_void;
    fn nros_platform_dealloc(ptr: *mut c_void);
}

/// Heap context for the boot (per-tier) app task.
struct AppContextTiers<F> {
    config: Config,
    tiers: &'static [TierSpec<'static>],
    setup: F,
}

/// Heap context handed to each spawned (non-boot) tier task.
struct TierTaskCtx<F> {
    session: ::nros::SessionHandle,
    tier: TierSpec<'static>,
    setup: F,
}

/// Spawned tier task: open an `Executor` over the shared session, install this
/// tier's `active_groups` filter, register (the off-tier callbacks are gated
/// out), then spin forever at the tier's period.
///
/// # Safety
/// `arg` is an `nros_platform_alloc`-allocated `TierTaskCtx<F>` from
/// `app_task_entry_tiers`; this task consumes + frees it.
unsafe extern "C" fn tier_task_entry<B, F, E>(arg: *mut c_void)
where
    B: BoardPrint + BoardExit,
    F: Fn(&mut RuntimeCtx<'_>) -> core::result::Result<(), E> + Copy,
    E: core::fmt::Debug,
{
    let ctx = unsafe { core::ptr::read(arg as *mut TierTaskCtx<F>) };
    unsafe { nros_platform_dealloc(arg) };

    // SAFETY: the boot task owns the session for the firmware lifetime (its spin
    // loop never returns), so the handle stays valid.
    let executor = unsafe { ::nros::Executor::open_with_session_handle(ctx.session) };
    let mut crt = ::nros::node_runtime::ExecutorNodeRuntime::from_executor(executor);
    crt.executor_mut().set_active_groups(ctx.tier.groups);
    {
        let mut runtime = RuntimeCtx::with_runtime(&mut crt);
        if let Err(e) = (ctx.setup)(&mut runtime) {
            B::println(format_args!(
                "nros: tier `{}` setup failed: {:?}",
                ctx.tier.name, e
            ));
            B::exit_failure();
        }
    }
    let period_ms = (ctx.tier.spin_period_us / 1000).max(1) as u32;
    loop {
        if let Err(err) = ::nros_platform::NodeDispatchRuntime::spin_once(&mut crt, period_ms) {
            unsafe {
                nros_trace_trigger_and_dump();
            }
            B::println(format_args!(
                "nros: tier `{}` spin error: {:?}",
                ctx.tier.name, err
            ));
            B::exit_failure();
        }
    }
}

/// Boot app task for the per-tier model: bring up the network, open the one
/// session, spawn one FreeRTOS task per non-boot tier (each sharing the session
/// via a `SessionHandle`), then run the highest-priority tier on this task.
///
/// # Safety
/// `arg` is an `nros_platform_alloc`-allocated `AppContextTiers<F>` from
/// `run_tiers_entry`, surviving until the scheduler exits.
unsafe extern "C" fn app_task_entry_tiers<B, F, E>(arg: *mut c_void)
where
    B: BoardPrint + BoardExit,
    F: Fn(&mut RuntimeCtx<'_>) -> core::result::Result<(), E> + Copy,
    E: core::fmt::Debug,
{
    let ctx = unsafe { &mut *(arg as *mut AppContextTiers<F>) };
    unsafe { freertos_boot_bringup::<B>(&ctx.config) };

    if ctx.tiers.is_empty() {
        B::println(format_args!("nros: run_tiers called with no tiers"));
        B::exit_failure();
    }

    // Open the one session on the boot task, then move it into its final
    // location (`crt`) BEFORE handing out `SessionHandle`s — the handle aliases
    // `crt`'s owned session, and `crt` never moves again (the boot spin loop
    // below never returns), so the spawned tasks' pointers stay valid.
    let exec_cfg = ::nros::ExecutorConfig::new(ctx.config.zenoh_locator)
        .domain_id(ctx.config.domain_id)
        .node_name("nros_app");
    let boot_exec = match ::nros::Executor::open(&exec_cfg) {
        Ok(e) => e,
        Err(err) => {
            unsafe {
                nros_trace_trigger_and_dump();
            }
            B::println(format_args!("Executor::open failed: {:?}", err));
            B::exit_failure();
        }
    };
    let mut crt = ::nros::node_runtime::ExecutorNodeRuntime::from_executor(boot_exec);

    // Spawn tiers[1..]; tiers[0] (highest priority) runs on this boot task.
    for tier in &ctx.tiers[1..] {
        let tier_ctx = TierTaskCtx::<F> {
            session: crt.executor_mut().session_handle(),
            tier: *tier,
            setup: ctx.setup,
        };
        let size = core::mem::size_of::<TierTaskCtx<F>>();
        let ptr = unsafe { nros_platform_alloc(size) as *mut TierTaskCtx<F> };
        if ptr.is_null() {
            B::println(format_args!("nros: tier `{}` ctx alloc failed", tier.name));
            B::exit_failure();
        }
        unsafe { core::ptr::write(ptr, tier_ctx) };
        // Raw per-RTOS priority (the author wrote the FreeRTOS value directly).
        let prio = tier.priority.clamp(0, u32::MAX as i64) as u32;
        let stack_words = if tier.stack_bytes == 0 {
            ctx.config.app_stack_bytes / 4
        } else {
            (tier.stack_bytes / 4) as u32
        };
        let ret = unsafe {
            nros_freertos_create_task(
                tier_task_entry::<B, F, E>,
                b"nros_tier\0".as_ptr(),
                stack_words,
                ptr as *mut c_void,
                prio,
            )
        };
        if ret != 0 {
            B::println(format_args!("nros: failed to spawn tier `{}`", tier.name));
            B::exit_failure();
        }
    }

    let boot_tier = ctx.tiers[0];
    crt.executor_mut().set_active_groups(boot_tier.groups);
    {
        let mut runtime = RuntimeCtx::with_runtime(&mut crt);
        if let Err(e) = (ctx.setup)(&mut runtime) {
            unsafe {
                nros_trace_trigger_and_dump();
            }
            B::println(format_args!("Application error: {:?}", e));
            B::exit_failure();
        }
    }
    B::println(format_args!(""));
    B::println(format_args!(
        "Multi-tier setup complete — entering boot-tier spin loop."
    ));
    let period_ms = (boot_tier.spin_period_us / 1000).max(1) as u32;
    loop {
        if let Err(err) = ::nros_platform::NodeDispatchRuntime::spin_once(&mut crt, period_ms) {
            unsafe {
                nros_trace_trigger_and_dump();
            }
            B::println(format_args!("spin_once error: {:?}", err));
            B::exit_failure();
        }
    }
}

/// Phase 228.E.2 — per-tier FreeRTOS entry. The `nros::main!()` macro emits
/// `<Board>::run_tiers(TIERS, run_plan)`; the board ZST routes here. Mirrors
/// [`run_entry`] but spawns one FreeRTOS task per priority tier over one shared
/// session (RFC-0032 §5; MT=1 is the default on FreeRTOS, §5.0). `tiers` are the
/// macro-baked `&'static [TierSpec]`; `setup` is the register-only `run_plan`
/// (invoked once per tier, hence `Fn + Copy`).
pub fn run_tiers_entry<B, F, E>(
    config: Config,
    tiers: &'static [TierSpec<'static>],
    setup: F,
) -> core::result::Result<(), E>
where
    B: BoardInit + BoardPrint + BoardExit,
    F: Fn(&mut RuntimeCtx<'_>) -> core::result::Result<(), E> + Copy,
    E: core::fmt::Debug,
{
    B::println(format_args!(""));
    B::println(format_args!("========================================"));
    B::println(format_args!("  nros FreeRTOS Platform (multi-tier)"));
    B::println(format_args!("========================================"));
    B::println(format_args!(""));

    B::init_hardware();

    let app_pri = Config::to_freertos_priority(config.app_priority);
    let app_stack_words = config.app_stack_bytes / 4;

    let ctx_ptr = unsafe {
        let size = core::mem::size_of::<AppContextTiers<F>>();
        let ptr = nros_platform_alloc(size) as *mut AppContextTiers<F>;
        assert!(!ptr.is_null(), "Failed to allocate AppContextTiers");
        core::ptr::write(
            ptr,
            AppContextTiers {
                config,
                tiers,
                setup,
            },
        );
        ptr
    };

    let ret = unsafe {
        nros_freertos_create_task(
            app_task_entry_tiers::<B, F, E>,
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

    B::exit_failure()
}

fn init_network(config: &Config) -> FrResult<()> {
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

/// Phase 212.N.2 — family-driver entry point for FreeRTOS boards.
///
/// Mirrors the legacy [`crate::run`] body — allocates an app task on
/// the FreeRTOS heap, hands it the user closure, calls
/// `vTaskStartScheduler()`, never returns — but routes through the
/// 212.N.1 `nros_platform::board` trait set + [`RuntimeCtx`].
///
/// Per-board crates (e.g. `nros-board-mps2-an385-freertos`) wire
/// this into their `impl BoardEntry for Self::run` body in 212.N.3:
///
/// ```ignore
/// impl nros_platform::board::BoardEntry for MyBoard {
///     fn run<F, E>(setup: F) -> Result<(), E>
///     where
///         F: FnOnce(&mut RuntimeCtx<'_>) -> Result<(), E>,
///         E: core::fmt::Debug,
///     {
///         let cfg = Config::default();
///         nros_board_freertos::run_entry::<MyBoard, F, E>(cfg, setup)
///     }
/// }
/// ```
///
/// # Type parameters
///
/// - `B: BoardInit + BoardPrint + BoardExit` — per-board glue
///   pulled from `nros_platform::board` (212.N.1 surface).
/// - `F: FnOnce(&mut RuntimeCtx<'_>) -> Result<(), E>` — user
///   closure receiving the runtime context.
/// - `E: core::fmt::Debug` — closure error type.
///
/// # Return
///
/// The signature is `Result<(), E>` to satisfy the
/// [`nros_platform::BoardEntry::run`] trait contract, but in
/// practice the kernel-spawn flow never returns to the caller —
/// either the scheduler runs forever and the app task drives
/// `exit_success` / `exit_failure`, or scheduler startup itself
/// fails and we `exit_failure` defensively. The `Ok(())` arm exists
/// only so the function signature lines up with the trait; it is
/// unreachable in a working build.
pub fn run_entry<B, F, E>(config: Config, setup: F) -> core::result::Result<(), E>
where
    B: BoardInit + BoardPrint + BoardExit,
    F: FnOnce(&mut RuntimeCtx<'_>) -> core::result::Result<(), E>,
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

    // Per-board pre-scheduler init. New 212.N.1 `BoardInit::init_hardware`
    // is parameterless — board crates read any needed config off their
    // own `pub const` / `pub static` rather than a passed-in arg.
    B::init_hardware();

    let app_pri = Config::to_freertos_priority(config.app_priority);
    let app_stack_words = config.app_stack_bytes / 4;

    // Heap-allocate the app context. Pre-scheduler MSP stack is
    // reclaimed by FreeRTOS when `vPortStartFirstTask()` resets MSP
    // to `_estack`, so locals would be clobbered by the next
    // exception that stacks on MSP. (Same rationale as legacy `run`.)
    // `nros_platform_alloc` is declared at module scope (heap_4 funnel).
    let ctx_ptr = unsafe {
        let size = core::mem::size_of::<AppContext<F>>();
        let ptr = nros_platform_alloc(size) as *mut AppContext<F>;
        assert!(!ptr.is_null(), "Failed to allocate AppContext");
        core::ptr::write(
            ptr,
            AppContext {
                config,
                closure: setup,
            },
        );
        ptr
    };

    let ret = unsafe {
        nros_freertos_create_task(
            app_task_entry_runtime::<B, F, E>,
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

    // Unreachable — scheduler never returns. `exit_failure()`
    // diverges (`-> !`), so this satisfies the `Result<(), E>`
    // signature without an explicit `Ok` arm.
    B::exit_failure()
}
