//! issue #128 (half 2) / RFC-0015 Model 1 — per-tier multi-task entry for
//! Zephyr: one `k_thread` per priority tier over ONE shared zenoh session.
//!
//! Mirrors `nros_board_freertos::run_tiers_entry` minus the network +
//! scheduler bring-up (Zephyr owns boot; `rust_main` — the caller — is
//! already a running post-init thread): the caller thread opens the boot
//! `Executor`, spawns `tiers[1..]` through the module's
//! `nros_zephyr_tier_task_create` shim (`k_thread_create` on a static pool,
//! RAW Zephyr priority — negatives = cooperative, exactly the
//! `[tiers.<name>.zephyr].priority` value), and then runs the
//! highest-priority tier itself. Each tier task opens an `Executor` over the
//! shared session (`SessionHandle`), installs its `active_groups` filter,
//! runs the SAME register-only setup closure (the groups gate what actually
//! registers), and spins forever at the tier's declared period.
//!
//! The `nros::main!` `Framework::Zephyr` arm emits
//! `ZephyrBoard::run_tiers(&config, TIERS, closure)` for multi-tier systems
//! (single-tier keeps the plain register+spin scaffold).

extern crate alloc;

use alloc::boxed::Box;
use core::ffi::c_void;

use nros_platform::{NodeDispatchRuntime, RuntimeCtx, RuntimeError, TierSpec};

use crate::ZephyrBoard;

unsafe extern "C" {
    /// `zephyr/nros_platform_zephyr_shims.c` — `k_thread_create` on a static
    /// tier pool at the RAW Zephyr priority. Returns 0 on success, -1 when
    /// the pool (`NROS_ZEPHYR_MAX_TIERS`) is exhausted.
    fn nros_zephyr_tier_task_create(
        entry: unsafe extern "C" fn(*mut c_void) -> *mut c_void,
        arg: *mut c_void,
        priority: i32,
        name: *const core::ffi::c_char,
    ) -> i32;
    /// Adopt a raw Zephyr priority on the CALLING thread — the boot thread
    /// runs `tiers[0]` itself, so it must take that tier's declared priority
    /// (`k_thread_priority_set(k_current_get(), …)`).
    fn nros_zephyr_set_current_priority(priority: i32);
}

/// Leaked per-tier context, consumed by [`tier_task_entry`].
struct TierTaskCtx<F> {
    session: ::nros::SessionHandle,
    tier: TierSpec<'static>,
    setup: F,
}

/// Spawned tier task: open an `Executor` over the shared session, install
/// this tier's `active_groups` filter, register (off-tier callbacks are
/// gated out), then spin forever at the tier's period. Never returns; on
/// setup/spin failure it logs and parks (the firmware equivalent of the
/// FreeRTOS `exit_failure`, which Zephyr application threads don't have).
unsafe extern "C" fn tier_task_entry<F>(arg: *mut c_void) -> *mut c_void
where
    F: Fn(&mut RuntimeCtx<'_>) -> Result<(), RuntimeError> + Copy + 'static,
{
    let ctx = unsafe { Box::from_raw(arg as *mut TierTaskCtx<F>) };
    // SAFETY: the boot thread owns the session for the firmware lifetime
    // (its spin loop never returns), so the handle stays valid.
    let executor = unsafe { ::nros::Executor::open_with_session_handle(ctx.session) };
    let mut crt = ::nros::node_runtime::ExecutorNodeRuntime::from_executor(executor);
    crt.executor_mut().set_active_groups(ctx.tier.groups);
    {
        let mut runtime = RuntimeCtx::with_runtime(&mut crt);
        if let Err(e) = (ctx.setup)(&mut runtime) {
            ::log::error!("nros: tier `{}` setup failed: {:?}", ctx.tier.name, e);
            loop {
                crate::zephyr_msleep(1000);
            }
        }
    }
    let period_ms = ((ctx.tier.spin_period_us / 1000).max(1)) as u32;
    loop {
        if let Err(err) = NodeDispatchRuntime::spin_once(&mut crt, period_ms) {
            ::log::error!("nros: tier `{}` spin error: {:?}", ctx.tier.name, err);
            loop {
                crate::zephyr_msleep(1000);
            }
        }
    }
}

impl ZephyrBoard {
    /// issue #128 (half 2) — per-tier multi-task entry. The caller (the
    /// `nros::main!` Zephyr arm's `rust_main` context) has already gated on
    /// the network, registered the RMW backend, and built `config` from the
    /// west-baked locator; this opens the ONE session, spawns `tiers[1..]`
    /// as `k_thread`s at their raw Zephyr priorities, and runs `tiers[0]`
    /// (highest priority) on the caller thread — never returns on success.
    pub fn run_tiers<F>(
        config: &::nros::ExecutorConfig,
        tiers: &'static [TierSpec<'static>],
        setup: F,
    ) -> Result<(), RuntimeError>
    where
        F: Fn(&mut RuntimeCtx<'_>) -> Result<(), RuntimeError> + Copy + 'static,
    {
        if tiers.is_empty() {
            ::log::error!("nros: run_tiers called with no tiers");
            return Err(RuntimeError::Spin);
        }

        let boot_exec = match ::nros::Executor::open(config) {
            Ok(e) => e,
            Err(err) => {
                ::log::error!("nros: zephyr tiers — executor open failed: {:?}", err);
                return Err(RuntimeError::Spin);
            }
        };
        let mut crt = ::nros::node_runtime::ExecutorNodeRuntime::from_executor(boot_exec);

        // Boot-tier setup FIRST, tier spawn after: entity declares carry an
        // interest handshake (the zenoh-pico write filter opens only when the
        // router's current-subscriber reply lands), and concurrent declares
        // from two threads race that handshake — the losing publisher's
        // filter stays closed and every put is silently dropped. Serializing
        // boot's declares before the spawned tiers' removes the boot↔tier
        // race; tiers[1..] still declare concurrently with the boot SPIN,
        // which only exchanges keepalives/data, not declares.
        let boot_tier = &tiers[0];
        crt.executor_mut().set_active_groups(boot_tier.groups);
        {
            let mut runtime = RuntimeCtx::with_runtime(&mut crt);
            setup(&mut runtime)?;
        }

        // Spawn tiers[1..]; tiers[0] runs on this (boot) thread. The ctx is
        // leaked into the spawned task, which reclaims it via `Box::from_raw`.
        for tier in &tiers[1..] {
            let ctx = Box::new(TierTaskCtx::<F> {
                session: crt.executor_mut().session_handle(),
                tier: *tier,
                setup,
            });
            let prio = tier.priority.clamp(i32::MIN as i64, i32::MAX as i64) as i32;
            let rc = unsafe {
                nros_zephyr_tier_task_create(
                    tier_task_entry::<F>,
                    Box::into_raw(ctx) as *mut c_void,
                    prio,
                    c"nros_tier".as_ptr(),
                )
            };
            if rc != 0 {
                ::log::error!(
                    "nros: failed to spawn tier `{}` (pool exhausted? NROS_ZEPHYR_MAX_TIERS)",
                    tier.name
                );
                return Err(RuntimeError::Spin);
            }
        }

        // The boot thread runs tiers[0] itself — adopt its declared raw
        // priority (the spawned tiers already got theirs at k_thread_create;
        // without this, tiers[0] would inherit the main-thread default and
        // the declared tier QoS would not hold for it).
        unsafe {
            nros_zephyr_set_current_priority(
                boot_tier.priority.clamp(i32::MIN as i64, i32::MAX as i64) as i32,
            );
        }
        ::log::info!(
            "nros: zephyr multi-tier entry up ({} tiers, boot tier `{}`)",
            tiers.len(),
            boot_tier.name
        );
        let period_ms = ((boot_tier.spin_period_us / 1000).max(1)) as u32;
        loop {
            if let Err(err) = NodeDispatchRuntime::spin_once(&mut crt, period_ms) {
                ::log::error!("nros: boot tier `{}` spin error: {:?}", boot_tier.name, err);
                return Err(RuntimeError::Spin);
            }
        }
    }
}
