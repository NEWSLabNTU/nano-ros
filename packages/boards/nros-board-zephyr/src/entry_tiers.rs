//! issue #128 (half 2) / RFC-0015 Model 1 — per-tier multi-task entry for
//! Zephyr: one `k_thread` per priority tier over ONE shared zenoh session.
//!
//! Mirrors `nros_board_freertos::run_tiers_entry` minus the network +
//! scheduler bring-up (Zephyr owns boot; `rust_main` — the caller — is
//! already a running post-init thread): the caller thread opens the boot
//! `Executor`, runs the boot tier's setup, then CHAIN-spawns the remaining
//! tiers (issue #144) through the module's `nros_zephyr_tier_task_create`
//! shim (`k_thread_create` on a static pool, RAW Zephyr priority — negatives
//! = cooperative, exactly the `[tiers.<name>.zephyr].priority` value), and
//! then runs the highest-priority tier itself. Each spawned tier task opens
//! an `Executor` over the shared session (`SessionHandle`), installs its
//! `active_groups` filter, runs the SAME register-only setup closure (the
//! groups gate what actually registers), spawns the NEXT tier once its own
//! setup returns, and spins forever at the tier's declared period. Chaining
//! spawns behind each setup keeps setup order total so no two entity-declare
//! bursts race the shared session's interest handshake.
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
    /// phase-296 W5.5 — apply an earliest-deadline (µs) on the CALLING
    /// thread via `k_thread_deadline_set`. Returns 1 when the kernel
    /// actually applied it (EDF present), 0 when it was a no-op (image
    /// lacks `CONFIG_SCHED_DEADLINE`).
    fn nros_zephyr_set_current_deadline(deadline_us: u32) -> i32;
}

/// Apply this tier's kernel EDF deadline on the CALLING thread, when the
/// tier is real-time and carries a deadline. Gated by the `zephyr-edf`
/// feature; off ⇒ the executor's cooperative `SchedContext` deadline
/// monitor is the sole enforcement (an honest Backfill). The marker is
/// logged ONLY when the shim reports the deadline was actually applied
/// (kernel has `CONFIG_SCHED_DEADLINE`) — else a kernel-less image could
/// log the marker while nothing was applied. The `::log::info!` literal
/// MUST match `nros_tests::output::ZEPHYR_EDF_DEADLINE_MARKER`.
#[cfg(feature = "zephyr-edf")]
fn apply_tier_deadline(tier: &TierSpec<'_>) {
    if tier.class == Some("real_time") {
        if let Some(us) = tier.deadline_us {
            let us = us.min(u32::MAX as u64) as u32;
            let applied = unsafe { nros_zephyr_set_current_deadline(us) };
            if applied != 0 {
                ::log::info!("nros: EDF deadline set tier=`{}` {}us", tier.name, us);
            }
        }
    }
}

#[cfg(not(feature = "zephyr-edf"))]
#[inline]
fn apply_tier_deadline(_tier: &TierSpec<'_>) {}

/// Leaked per-tier context, consumed by [`tier_task_entry`].
struct TierTaskCtx<F> {
    session: ::nros::SessionHandle,
    tier: TierSpec<'static>,
    /// Tiers still to spawn AFTER this one — the chained-spawn tail
    /// (issue #144). This tier spawns `rest[0]` (carrying `rest[1..]`)
    /// only after its OWN setup returns, so no two setups overlap.
    rest: &'static [TierSpec<'static>],
    setup: F,
}

/// issue #144 — chained tier spawn. `remaining.split_first()` is the next
/// tier to bring up and the tail it must carry; empty → nothing left, `Ok`.
/// Spawns exactly ONE `k_thread` for `remaining[0]`, handing it `remaining[1..]`
/// as its own `rest` so the spawn chain continues once its setup completes.
/// Serializing spawns behind each setup guarantees no two `setup()` (entity
/// declare) calls ever run concurrently on the shared zenoh-pico session — the
/// interest-handshake race that silently closes a losing publisher's write
/// filter under `Z_FEATURE_MULTI_THREAD=0`.
fn spawn_next_tier<F>(
    session: ::nros::SessionHandle,
    remaining: &'static [TierSpec<'static>],
    setup: F,
) -> Result<(), RuntimeError>
where
    F: Fn(&mut RuntimeCtx<'_>) -> Result<(), RuntimeError> + Copy + 'static,
{
    let Some((tier, rest)) = remaining.split_first() else {
        return Ok(());
    };
    let ctx = Box::new(TierTaskCtx::<F> {
        session,
        tier: *tier,
        rest,
        setup,
    });
    let prio = tier.priority.clamp(i32::MIN as i64, i32::MAX as i64) as i32;
    // Hold the raw pointer so a failed create can reclaim it — the task never
    // runs, so `tier_task_entry`'s `Box::from_raw` never fires; without this
    // the `TierTaskCtx` heap block leaks for the firmware lifetime.
    let raw = Box::into_raw(ctx);
    let rc = unsafe {
        nros_zephyr_tier_task_create(
            tier_task_entry::<F>,
            raw as *mut c_void,
            prio,
            c"nros_tier".as_ptr(),
        )
    };
    if rc != 0 {
        // SAFETY: the create failed, so ownership of `raw` was not transferred
        // to a task; reclaim + drop it here.
        drop(unsafe { Box::from_raw(raw) });
        ::log::error!(
            "nros: failed to spawn tier `{}` (pool exhausted? NROS_ZEPHYR_MAX_TIERS)",
            tier.name
        );
        return Err(RuntimeError::Spin);
    }
    Ok(())
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
    // W5.4 — lower this tier's class/budget/period/deadline onto the executor's
    // default SchedContext (Sporadic / EDF / TT), shared with every board.
    crt.apply_tier_sched_policy(
        ctx.tier.class,
        ctx.tier.period_us,
        ctx.tier.budget_us,
        ctx.tier.deadline_us,
        ctx.tier.deadline_policy,
    );
    apply_tier_deadline(&ctx.tier);
    {
        let mut runtime = RuntimeCtx::with_runtime(&mut crt);
        if let Err(e) = (ctx.setup)(&mut runtime) {
            // The chain is serialized (issue #144): this tier spawns the next
            // only AFTER its own setup returns Ok, so a setup failure here HALTS
            // the chain — `ctx.rest` (this tier's downstream tiers) will not
            // start. That is intentional (a tier whose baked config can't
            // declare its entities means a degraded deploy anyway), but say so
            // loudly rather than leaving the downstream tiers silently absent.
            ::log::error!(
                "nros: tier `{}` setup failed: {:?} — {} downstream tier(s) will NOT start",
                ctx.tier.name,
                e,
                ctx.rest.len()
            );
            loop {
                crate::zephyr_msleep(1000);
            }
        }
    }
    // issue #144 — this tier's setup is done, so it is now safe to bring up the
    // next tier: spawn `rest[0]` (carrying `rest[1..]`). Mint a fresh handle off
    // this tier's executor (same as the boot path — `ctx.session` was consumed
    // opening the executor above). A failed DOWNSTREAM spawn must NOT stop this
    // tier spinning its own work, so log + continue.
    let next_session = crt.executor_mut().session_handle();
    if let Err(e) = spawn_next_tier(next_session, ctx.rest, ctx.setup) {
        ::log::error!(
            "nros: tier `{}` failed to spawn next tier: {:?}",
            ctx.tier.name,
            e
        );
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
        // filter stays closed and every put is silently dropped. issue #144 —
        // serializing boot's declares before ANY spawn removes the boot↔tier
        // race, and CHAINING the remaining spawns (boot spawns tiers[1] only;
        // each tier spawns the next after its own setup returns) removes the
        // tier↔tier race too: setup order is total (boot, t1, t2, …), no two
        // declares ever overlap. Spins still overlap the next tier's setup,
        // which is SAFE — a spin exchanges keepalives/data, not declares (only
        // declare-vs-declare races the interest handshake).
        let boot_tier = &tiers[0];
        crt.executor_mut().set_active_groups(boot_tier.groups);
        crt.apply_tier_sched_policy(
            boot_tier.class,
            boot_tier.period_us,
            boot_tier.budget_us,
            boot_tier.deadline_us,
            boot_tier.deadline_policy,
        );
        {
            let mut runtime = RuntimeCtx::with_runtime(&mut crt);
            setup(&mut runtime)?;
        }

        // Kick off the chain: spawn tiers[1] carrying tiers[2..] as its tail;
        // tiers[0] runs on this (boot) thread. A boot-side spawn failure is
        // fatal (takes the error/exit path) — unlike a downstream tier's.
        spawn_next_tier(crt.executor_mut().session_handle(), &tiers[1..], setup)?;

        // The boot thread runs tiers[0] itself — adopt its declared raw
        // priority (the spawned tiers already got theirs at k_thread_create;
        // without this, tiers[0] would inherit the main-thread default and
        // the declared tier QoS would not hold for it).
        unsafe {
            nros_zephyr_set_current_priority(
                boot_tier.priority.clamp(i32::MIN as i64, i32::MAX as i64) as i32,
            );
        }
        apply_tier_deadline(boot_tier);
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
