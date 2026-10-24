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
//! then runs `tiers[0]` itself. NOTE (issue 0251): `resolve_tiers` sorts
//! descending by RAW number without inverting per RTOS direction, and Zephyr
//! is lower-number-wins — so `tiers[0]`, the boot tier, is the
//! numerically-largest = LOWEST-priority tier. Each spawned tier task opens
//! an `Executor` over the shared session (`SessionHandle`), installs its
//! `active_groups` filter, runs the SAME register-only setup closure (the
//! groups gate what actually registers), spawns the NEXT tier once its own
//! setup returns, and spins forever at the tier's declared period. Chaining
//! spawns behind each setup keeps setup order total so no two entity-declare
//! bursts race the shared session's interest handshake.
//!
//! phase-509 P10 (RFC-0108 "As built in P10"): the tiers are LEVELS over the
//! core's ONE executor (`nros::levels::LevelCore`), no longer an executor per
//! tier. The boot level owns the executor and the session; each spawned level
//! is a worker thread that opens its level, runs the setup closure on the
//! shared runtime under the executor's lock (a bare `k_mutex`, the C runner's),
//! and loops on its band's passes. A level that does not open, whose setup
//! fails or that cannot be spawned fails the boot (`nros_zephyr_fail_boot`).
//!
//! The `nros::main!` `Framework::Zephyr` arm emits
//! `ZephyrBoard::run_tiers(&config, TIERS, TIER_BACKING, closure)` for multi-tier systems
//! (single-tier keeps the plain register+spin scaffold).

extern crate alloc;

use alloc::boxed::Box;
use core::ffi::c_void;

use nros_platform::{RuntimeCtx, RuntimeError, TierSpec};

use crate::ZephyrBoard;

unsafe extern "C" {
    /// `zephyr/nros_platform_zephyr_shims.c` — `k_thread_create` on a static
    /// tier pool at the RAW Zephyr priority. Returns 0 on success, -1 when
    /// the pool (`NROS_ZEPHYR_MAX_TIERS`) is exhausted.
    /// issue 0655 — `core_plus1` (0 = unpinned) makes the shim create the
    /// thread SUSPENDED and pin it before starting, the only window in which
    /// Zephyr accepts a cpu mask. The kernel's own return code comes back in
    /// `pin_rc` so the marker text stays here, in lockstep with
    /// `nros_tests::output::ZEPHYR_CORE_PIN_*`.
    fn nros_zephyr_tier_task_create(
        entry: unsafe extern "C" fn(*mut c_void) -> *mut c_void,
        arg: *mut c_void,
        priority: i32,
        name: *const core::ffi::c_char,
        stack_bytes: usize,
        core_plus1: u32,
        pin_rc: *mut i32,
    ) -> i32;
    /// issue 1232 — the same spawn over the ENTRY's stack (a `TierTaskMemory`
    /// static sized from the tier's declared `stack_bytes`); the thread object
    /// still comes from the pool. -1 when refused (see the shim).
    fn nros_zephyr_tier_task_create_stack(
        entry: unsafe extern "C" fn(*mut c_void) -> *mut c_void,
        arg: *mut c_void,
        priority: i32,
        name: *const core::ffi::c_char,
        stack: *mut u8,
        stack_bytes: usize,
        core_plus1: u32,
        pin_rc: *mut i32,
    ) -> i32;
    /// issue 1232 — the CALLING thread's stack as the kernel recorded it, or 0
    /// when the image keeps no stack info.
    fn nros_zephyr_current_stack_size() -> usize;
    /// Adopt a raw Zephyr priority on the CALLING thread — the boot thread
    /// runs `tiers[0]` itself, so it must take that tier's declared priority
    /// (`k_thread_priority_set(k_current_get(), …)`).
    fn nros_zephyr_set_current_priority(priority: i32);
    /// phase-296 W5.5 — apply an earliest-deadline (µs) on the CALLING
    /// thread via `k_thread_deadline_set`. Returns 1 when the kernel
    /// actually applied it (EDF present), 0 when it was a no-op (image
    /// lacks `CONFIG_SCHED_DEADLINE`).
    fn nros_zephyr_set_current_deadline(deadline_us: u32) -> i32;
    /// Phase 110.D shim — pin the CALLING thread to a CPU
    /// (`k_thread_cpu_pin`). Returns 0 on success, `-ENOSYS` when the image
    /// lacks `CONFIG_SCHED_CPU_MASK_PIN_ONLY`, else the kernel error.
    fn nros_zephyr_thread_cpu_pin(cpu: i32) -> i32;
    /// issue 0260 — the CPU the CALLING thread is observed on, or
    /// `NROS_ZEPHYR_CPU_UNKNOWN` when the image cannot say (no `CONFIG_SMP`;
    /// the posix arch does not provide `arch_proc_id` at all).
    fn nros_zephyr_current_cpu() -> u32;
    /// issue 1537 — `zephyr/nros_platform_zephyr_shims.c`. Judges ONE tier's
    /// RAW k_thread priority against the priorities the kernel actually gave
    /// the platform's explicitly-prioritised tasks (the zenoh transport), and
    /// prints the report on the first tier that meets or outranks them — or,
    /// once, that the image has no band to judge against. Returns 1 / 0 / -1.
    fn nros_zephyr_report_tier_vs_transport(
        tier: *const core::ffi::c_char,
        tier_len: usize,
        prio: i32,
    ) -> i32;
}

/// issue 1537 — the Zephyr twin of `nros_board_freertos`'s
/// `report_tiers_above_transport` (issue 0623): a derived or authored tier at
/// or above the transport is REPORTED at boot, whatever road baked it. The
/// logic lives in C, shared with `zephyr_run_tiers.c`, so the two arms cannot
/// disagree about what an inversion is.
fn report_tiers_above_transport(tiers: &[TierSpec<'_>]) {
    for tier in tiers {
        unsafe {
            nros_zephyr_report_tier_vs_transport(
                tier.name.as_ptr().cast(),
                tier.name.len(),
                tier.priority.clamp(i32::MIN as i64, i32::MAX as i64) as i32,
            );
        }
    }
}

/// Mirrors `NROS_ZEPHYR_CPU_UNKNOWN` in `nros_platform_zephyr_shims.c`.
const CPU_UNKNOWN: u32 = u32::MAX;

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

/// phase-436 W7 — Zephyr's microsecond park (`k_sem_take` with `K_USEC`) and
/// the tick period that is its real floor. Optional in the platform ABI, so a
/// build without them simply keeps the millisecond `wake_wait_ms` path.
unsafe extern "C" {
    fn nros_platform_wake_park_until_us(w: *mut core::ffi::c_void, deadline_us: u64) -> i8;
    fn nros_platform_wake_park_granularity_us() -> u64;
}

/// Install the park on `crt`'s executor. Advisory: a build with no wake object
/// keeps the path it had.
fn install_park(crt: &mut ::nros::node_runtime::ExecutorNodeRuntime) {
    let granularity = unsafe { nros_platform_wake_park_granularity_us() };
    let _ = ::nros::port_park::install_port_park(
        crt.executor_mut(),
        nros_platform_wake_park_until_us,
        granularity,
    );
}

/// The BOOT tier's placement attempt (issue 0655).
///
/// phase-296 W5.5 added this as a general "pin the calling thread" consumer,
/// used by both the boot tier and every spawned tier. It could never work:
/// Zephyr's `cpu_mask_mod` accepts a mask only on a thread that is prevented
/// from running, and `k_current_get()` never is. Spawned tiers now pin in the
/// create→start window inside `nros_zephyr_tier_task_create`; this path remains
/// only for the boot tier, which the kernel started before `run_tiers` ever saw
/// it and which therefore has no such window.
///
/// So a `core` declared on the boot tier CANNOT be honored on Zephyr. That is a
/// real limitation of the API, not a missing feature here, and it is reported
/// as one — RFC-0052's contract is that a dropped declaration is loud, and a
/// limitation stated plainly is more useful than a retry that cannot succeed.
fn apply_boot_tier_core_pin(tier: &TierSpec<'_>) {
    if let Some(cpu) = tier.core {
        let rc = unsafe { nros_zephyr_thread_cpu_pin(cpu.min(i32::MAX as u32) as i32) };
        if rc == 0 {
            // Not reachable on any Zephyr this pins against, but the accept arm
            // stays: if a future kernel permits a running-thread mask, this is
            // the honest marker for it rather than a stale failure note.
            ::log::info!("nros: core pin tier=`{}` cpu={}", tier.name, cpu);
        } else {
            ::log::warn!(
                "nros: core pin FAILED tier=`{}` cpu={} rc={} — this is the BOOT tier, which \
                 Zephyr started before nros ran, and a cpu mask is only settable before a \
                 thread starts (issue 0655). Declare the `core` on a SPAWNED tier, which is \
                 pinned in its create->start window. -88/ENOSYS instead means the image lacks \
                 CONFIG_SCHED_CPU_MASK. Tier runs unpinned",
                tier.name,
                cpu,
                rc
            );
        }
    }
}

// phase-509 P10 (RFC-0108 "As built in P9b") -- the executor's lock: a bare
// `k_mutex` (recursive, priority-inheriting) through the module's shims, the
// C runner's (`zephyr_run_tiers.c`), and the calling thread's identity.
unsafe extern "C" {
    fn nros_zephyr_exec_lock_size() -> usize;
    fn nros_zephyr_exec_lock_init(m: *mut c_void) -> i8;
    fn nros_zephyr_exec_lock(m: *mut c_void) -> i8;
    fn nros_zephyr_exec_unlock(m: *mut c_void) -> i8;
    fn nros_zephyr_thread_id() -> usize;
}

/// The core's executor lock, one per image: room for a `struct k_mutex`
/// (20-32 bytes on the targets), whose real size is checked at boot.
struct ExecMutex(core::cell::UnsafeCell<[u64; 8]>);
// SAFETY: handed to the kernel once, at boot, before any level thread exists.
unsafe impl Sync for ExecMutex {}
static EXEC_MUTEX: ExecMutex = ExecMutex(core::cell::UnsafeCell::new([0; 8]));

unsafe extern "C" fn exec_thread_id() -> usize {
    unsafe { nros_zephyr_thread_id() }
}

/// One spawned level's context, carried into its thread.
struct LevelTaskCtx<F> {
    core: ::nros::levels::LevelCore,
    tier: TierSpec<'static>,
    rest: &'static [TierSpec<'static>],
    rest_task_memory: &'static [::nros::TierTaskMemoryRaw],
    all: &'static [TierSpec<'static>],
    level: u8,
    setup: F,
}

/// The level of `remaining[0]` among `all`: its urgency order, Zephyr's
/// smaller-is-urgent (the rule the generated `NROS_ENTRY_LEVEL_OF_<node>`
/// uses).
fn level_of_suffix(all: &[TierSpec<'_>], remaining: &[TierSpec<'_>]) -> u8 {
    ::nros::tier_level_by(
        all.len(),
        all.len().saturating_sub(remaining.len()),
        true,
        |i| all[i].priority,
    )
    .unwrap_or(0)
}

/// Fail the boot: a level that did not open, whose setup failed or that could
/// not be spawned stops the image (the C runner's `nros_zephyr_fail_boot`,
/// `k_panic`). Parking left it half up with nothing to say so.
fn fail_boot(tier: &str, what: &str) -> ! {
    ::log::error!("nros: level `{}` {}: failing the boot", tier, what);
    unsafe extern "C" {
        fn nros_zephyr_fail_boot();
    }
    unsafe { nros_zephyr_fail_boot() };
    loop {
        crate::zephyr_msleep(1000);
    }
}

/// issue #144 -- chained level spawn: ONE `k_thread` for `remaining[0]`,
/// carrying `remaining[1..]`, so the setups never overlap. A level that cannot
/// be spawned fails the boot.
fn spawn_next_level<F>(
    core: ::nros::levels::LevelCore,
    all: &'static [TierSpec<'static>],
    remaining: &'static [TierSpec<'static>],
    task_memory: &'static [::nros::TierTaskMemoryRaw],
    setup: F,
) where
    F: Fn(&mut RuntimeCtx<'_>) -> Result<(), RuntimeError> + Copy + 'static,
{
    let Some((tier, rest)) = remaining.split_first() else {
        return;
    };
    let (memory, rest_task_memory) = match task_memory.split_first() {
        Some((m, rest_m)) => ((!m.stack.is_null()).then_some(*m), rest_m),
        None => (None, task_memory),
    };
    let raw = Box::into_raw(Box::new(LevelTaskCtx::<F> {
        core,
        tier: *tier,
        rest,
        rest_task_memory,
        all,
        level: level_of_suffix(all, remaining),
        setup,
    }));
    let prio = tier.priority.clamp(i32::MIN as i64, i32::MAX as i64) as i32;
    let core_plus1 = tier.core.map(|c| c.saturating_add(1)).unwrap_or(0);
    let mut pin_rc: i32 = 0;
    let rc = match memory {
        Some(m) => unsafe {
            nros_zephyr_tier_task_create_stack(
                level_task_entry::<F>,
                raw as *mut c_void,
                prio,
                c"nros_level".as_ptr(),
                m.stack,
                m.stack_bytes,
                core_plus1,
                &mut pin_rc,
            )
        },
        None => unsafe {
            nros_zephyr_tier_task_create(
                level_task_entry::<F>,
                raw as *mut c_void,
                prio,
                c"nros_level".as_ptr(),
                tier.stack_bytes,
                core_plus1,
                &mut pin_rc,
            )
        },
    };
    if rc != 0 {
        drop(unsafe { Box::from_raw(raw) });
        fail_boot(
            tier.name,
            "was not spawned (pool exhausted? CONFIG_NROS_ZEPHYR_MAX_TIERS)",
        );
    }
    if let Some(cpu) = tier.core {
        report_core_pin(tier.name, cpu, pin_rc);
    }
}

fn report_core_pin(name: &str, cpu: u32, rc: i32) {
    if rc == 0 {
        ::log::info!("nros: core pin tier=`{}` cpu={}", name, cpu);
    } else {
        ::log::warn!(
            "nros: core pin FAILED tier=`{}` cpu={} rc={} (-22/EINVAL: the thread was already \
             RUNNING — Zephyr accepts a cpu mask only before start, issue 0655; -88/ENOSYS: \
             image lacks CONFIG_SCHED_CPU_MASK) — tier runs unpinned",
            name,
            cpu,
            rc
        );
    }
}

/// A spawned LEVEL's thread: a worker over the core's one executor. It opens
/// its level, runs the image's setup on the shared runtime with its groups
/// active (what registers is this level's), spawns the next level, then loops
/// on its band's passes.
unsafe extern "C" fn level_task_entry<F>(arg: *mut c_void) -> *mut c_void
where
    F: Fn(&mut RuntimeCtx<'_>) -> Result<(), RuntimeError> + Copy + 'static,
{
    let LevelTaskCtx {
        core,
        tier,
        rest,
        rest_task_memory,
        all,
        level,
        setup,
    } = *unsafe { Box::from_raw(arg as *mut LevelTaskCtx<F>) };
    ::log::info!(
        "nros: tier stack tier=`{}` declared={} kernel={}",
        tier.name,
        tier.stack_bytes,
        unsafe { nros_zephyr_current_stack_size() }
    );
    apply_tier_deadline(&tier);
    {
        let observed = unsafe { nros_zephyr_current_cpu() };
        if observed != CPU_UNKNOWN {
            ::log::info!(
                "nros: core pin observed tier=`{}` running_on={}",
                tier.name,
                observed
            );
        }
    }
    // SAFETY: once, on this level's own thread.
    let worker = match unsafe { core.open_level(level) } {
        Ok(w) => w,
        Err(e) => fail_boot(
            tier.name,
            if e == ::nros::levels::LevelError::BadLevel {
                "did not open (level out of range)"
            } else {
                "did not open"
            },
        ),
    };
    // SAFETY: the runtime reference lives inside the closure only.
    let rc = unsafe {
        core.with_runtime(|crt| {
            let _ = crt.executor_mut().set_active_groups(tier.groups);
            let mut runtime = RuntimeCtx::with_runtime(crt);
            setup(&mut runtime)
        })
    };
    if let Err(e) = rc {
        ::log::error!("nros: level `{}` setup failed: {:?}", tier.name, e);
        fail_boot(tier.name, "setup failed");
    }
    spawn_next_level(core, all, rest, rest_task_memory, setup);
    let period = core::time::Duration::from_millis((tier.spin_period_us / 1000).max(1));
    let mut gap = ::nros_platform::TierSpinGap::new(tier.spin_period_us);
    loop {
        let iter = gap.mark();
        // SAFETY: `worker` is this thread's, from `open_level`.
        unsafe { core.spin_level_once(worker, period) };
        gap.after_spin(iter);
    }
}

impl ZephyrBoard {
    /// issue #128 (half 2) — the tiered entry. The caller (the `nros::main!`
    /// Zephyr arm's `rust_main` context) has already gated on the network,
    /// registered the RMW backend, and built `config` from the west-baked
    /// locator. phase-509 P10 (RFC-0108 "As built in P9b"): each tier is one
    /// LEVEL over the core's ONE executor; `tiers[0]` (the numerically-largest
    /// = LOWEST Zephyr priority, issue 0251) runs on the caller thread and
    /// owns the session, and every other level is a worker `k_thread` over the
    /// same executor at its raw Zephyr priority. Never returns on success.
    pub fn run_tiers<F>(
        config: &::nros::ExecutorConfig,
        tiers: &'static [TierSpec<'static>],
        tier_backing: &'static mut [::nros::TierExecutorBackingSlot],
        setup: F,
    ) -> Result<(), RuntimeError>
    where
        F: Fn(&mut RuntimeCtx<'_>) -> Result<(), RuntimeError> + Copy + 'static,
    {
        // No task memory: every spawned level on the pool slot, the shape an
        // entry generated before issue 1232 has.
        Self::run_tiers_with_task_memory(config, tiers, tier_backing, &[], setup)
    }

    /// issue 1232 — [`run_tiers`](Self::run_tiers) plus each spawned level's
    /// STACK from the entry: `task_memory[k]` is `tiers[k + 1]`'s (the boot
    /// level, `tiers[0]`, runs on this thread), a `TierTaskMemory` static sized
    /// from that tier's declared `stack_bytes`, or `TierTaskMemoryRaw::NONE`
    /// for a tier that declared none (it keeps the pool slot,
    /// CONFIG_NROS_ZEPHYR_TIER_STACK_SIZE). Refused unless empty or exactly one
    /// row per spawned level.
    ///
    /// `tier_backing` is still checked against the table (issue 1571) so an
    /// entry generated for another table is refused, but one executor serves
    /// every level now: the slots past the boot level's are unused.
    pub fn run_tiers_with_task_memory<F>(
        config: &::nros::ExecutorConfig,
        tiers: &'static [TierSpec<'static>],
        tier_backing: &'static mut [::nros::TierExecutorBackingSlot],
        task_memory: &'static [::nros::TierTaskMemoryRaw],
        setup: F,
    ) -> Result<(), RuntimeError>
    where
        F: Fn(&mut RuntimeCtx<'_>) -> Result<(), RuntimeError> + Copy + 'static,
    {
        if !task_memory.is_empty() && task_memory.len() != tiers.len().saturating_sub(1) {
            ::log::error!(
                "nros: tier task memory holds {} row(s) but {} tier(s) are spawned — the \
                 entry was generated for a different tier table (issue 1232)",
                task_memory.len(),
                tiers.len().saturating_sub(1)
            );
            return Err(RuntimeError::Spin);
        }
        ::nros_platform_cffi::log::init_default();

        // issue 0758 W4 — acquire the wall-clock epoch HERE, in the one window
        // where both halves of the ordering hold: the network is up and no
        // component has constructed yet. Same call, same point, as the C arm.
        unsafe {
            unsafe extern "C" {
                fn nros_zephyr_epoch_acquire_configured();
            }
            nros_zephyr_epoch_acquire_configured();
        }

        if tiers.is_empty() {
            ::log::error!("nros: run_tiers called with no tiers");
            return Err(RuntimeError::Spin);
        }
        if tiers.len() > ::nros::MAX_LEVELS {
            ::log::error!(
                "nros: {} tiers is past the {} levels a core holds",
                tiers.len(),
                ::nros::MAX_LEVELS
            );
            return Err(RuntimeError::Spin);
        }
        if let Err(short) = ::nros::check_tier_executor_backing(tiers.len(), tier_backing) {
            ::log::error!("nros: {}", short);
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

        // issue 1537 — the session is open; judge every tier against the
        // transport's priorities before any tier runs.
        report_tiers_above_transport(tiers);

        let boot_tier = &tiers[0];
        let n_levels = u8::try_from(tiers.len()).unwrap_or(u8::MAX);
        let boot_level = level_of_suffix(tiers, tiers);
        let _ = crt.executor_mut().set_active_groups(boot_tier.groups);
        install_park(&mut crt);
        crt.apply_tier_sched_policy(
            boot_tier.class,
            boot_tier.period_us,
            boot_tier.budget_us,
            boot_tier.deadline_us,
            boot_tier.deadline_policy,
        );
        // phase-509 P10 -- the core's level runtime: the lock, before any
        // second thread can reach the executor (one level needs none).
        let mutex = EXEC_MUTEX.0.get() as *mut c_void;
        if tiers.len() > 1
            && (unsafe { nros_zephyr_exec_lock_size() } > core::mem::size_of::<[u64; 8]>()
                || unsafe { nros_zephyr_exec_lock_init(mutex) } != 0)
        {
            ::log::error!("nros: zephyr run_levels: the executor lock was not installed; refused");
            return Err(RuntimeError::Spin);
        }
        // SAFETY: no second thread exists yet; the mutex is a static the
        // kernel initialised above; `k_current_get` names the calling thread.
        let core = match unsafe {
            ::nros::levels::LevelCore::new(
                crt,
                n_levels,
                boot_level,
                mutex,
                nros_zephyr_exec_lock,
                nros_zephyr_exec_unlock,
                exec_thread_id,
            )
        } {
            Ok(c) => c,
            Err(e) => {
                ::log::error!(
                    "nros: zephyr run_levels: the level runtime refused: {:?}",
                    e
                );
                return Err(RuntimeError::Spin);
            }
        };
        ::log::info!("nros: scheduler core=0 levels={}", n_levels);

        // issue #144 — the boot level's setup runs FIRST, then the levels
        // chain, so no two setups overlap on the shared session.
        // SAFETY: the runtime reference lives inside the closure only.
        let rc = unsafe {
            core.with_runtime(|crt| {
                let mut runtime = RuntimeCtx::with_runtime(crt);
                setup(&mut runtime)
            })
        };
        if let Err(e) = rc {
            ::log::error!("nros: level `{}` setup failed: {:?}", boot_tier.name, e);
            fail_boot(boot_tier.name, "setup failed");
        }

        spawn_next_level(core, tiers, &tiers[1..], task_memory, setup);

        unsafe {
            nros_zephyr_set_current_priority(
                boot_tier.priority.clamp(i32::MIN as i64, i32::MAX as i64) as i32,
            );
        }
        apply_tier_deadline(boot_tier);
        apply_boot_tier_core_pin(boot_tier);
        ::log::info!(
            "nros: zephyr multi-tier entry up ({} tiers, boot tier `{}`)",
            tiers.len(),
            boot_tier.name
        );
        ::log::info!(
            "nros: multi-level run — {} level(s) over one executor",
            tiers.len()
        );
        let period = core::time::Duration::from_millis((boot_tier.spin_period_us / 1000).max(1));
        let mut gap = ::nros_platform::TierSpinGap::new(boot_tier.spin_period_us);
        loop {
            let iter = gap.mark();
            // SAFETY: this is the boot level's thread.
            unsafe { core.spin_boot_once(period) };
            gap.after_spin(iter);
        }
    }
}
