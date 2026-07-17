//! Phase 110.B — `SchedContext` API + supporting types.
//!
//! A `SchedContext` is a first-class scheduling capability. Multiple
//! callbacks share one SC; one OS priority slot per Executor regardless
//! of callback count. Inspired by seL4 MCS (Mixed-Criticality
//! Scheduling).
//!
//! 110.B.a (this commit) lands the type surface + `EdfReadySet`. The
//! Executor builder methods (`create_sched_context`,
//! `register_subscription_in`, ...) and the cbindgen / C / C++ wrappers
//! land in 110.B.b once the const-generic `Executor<MAX_HANDLES,
//! MAX_SC>` reshape is sorted.

use core::num::NonZeroU32;

/// Optional time field with a sentinel `0` for "absent".
///
/// Phase 110.B keeps a stable `#[repr(transparent)]` u32 layout so
/// cbindgen emits plain `uint32_t` for C consumers — `Option<NonZeroU32>`
/// loses its niche optimization the moment a `#[repr(C)]` struct
/// embeds it. Rust callers see the ergonomic
/// [`get`](OptUs::get)-returning-`Option<NonZeroU32>` getter.
///
/// Sentinel `0` is physically meaningful for every time field on
/// [`SchedContext`]: 0-period would mean infinite frequency, 0-budget
/// means unbounded, 0-deadline means no deadline.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(transparent)]
pub struct OptUs(u32);

impl OptUs {
    pub const NONE: Self = Self(0);

    pub const fn from_us(us: u32) -> Self {
        Self(us)
    }

    pub const fn from_nz(nz: NonZeroU32) -> Self {
        Self(nz.get())
    }

    /// Returns the inner value or `None` when the sentinel is set.
    pub const fn get(self) -> Option<NonZeroU32> {
        NonZeroU32::new(self.0)
    }

    pub const fn is_some(self) -> bool {
        self.0 != 0
    }

    pub const fn raw(self) -> u32 {
        self.0
    }
}

/// Scheduling class — picks the runtime queue + selection policy for
/// the contained callbacks.
///
/// Phase 110.A only exercises `Fifo`; `Edf` lands with the
/// `EdfReadySet` plumb-up in 110.B.b; `Sporadic` is post-v1 (110.E);
/// `TimeTriggered` is post-v1 (110.G).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SchedClass {
    #[default]
    Fifo,
    Edf,
    Sporadic,
    BestEffort,
    /// Deprecated as of Phase 110.G refactor — TT is now an
    /// orthogonal slot-membership annotation via
    /// `SchedContext.tt_window_offset_us` /
    /// `tt_window_duration_us`, not a class. Keeping the variant
    /// for one release so exhaustive matches don't break; treated
    /// as `Fifo` in dispatch.
    #[deprecated(
        since = "0.1.0",
        note = "use SchedContext.tt_window_offset_us + tt_window_duration_us instead; \
                TT now cooperates with Fifo / Edf / Sporadic / BestEffort classes"
    )]
    TimeTriggered,
}

/// Criticality bucket for [`SchedContext`]. Phase 110.C uses this to
/// pick which `BucketedFifoSet` / `BucketedEdfSet` slot a callback
/// dispatches through; later phases (110.D) map it to OS priority.
///
/// Default `Normal` keeps existing single-bucket workloads unchanged
/// — every default-Fifo SC sits in `Normal`, so dispatch order is
/// bit-identical to pre-110.C when no callback opts in to `Critical`
/// or `BestEffort`.
///
/// Single-thread non-preemption note: a `BestEffort` callback already
/// running blocks `Critical` work that becomes ready mid-cycle. Hard-
/// RT scenarios need 110.D's multi-executor preemption.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Ord, PartialOrd)]
pub enum Priority {
    /// Highest-priority bucket. Drained first within a single
    /// `spin_once` cycle; non-preemptive against in-flight lower-
    /// priority callbacks (see Phase 110.D for preemption).
    Critical = 0,
    /// Default bucket. Most callbacks (and the auto-default Fifo SC)
    /// live here.
    #[default]
    Normal = 1,
    /// Lowest-priority bucket. Drained last; first to be skipped if a
    /// future cycle-budget overrun forces an early return.
    BestEffort = 2,
}

impl Priority {
    pub const COUNT: usize = 3;

    pub const fn index(self) -> usize {
        self as usize
    }
}

/// How an EDF deadline is interpreted relative to a callback firing.
///
/// - `Released`: deadline is `release_time + period`. Default for
///   timer-triggered callbacks.
/// - `Activated`: deadline is `activation_time + relative_deadline`.
///   Default for event-triggered subscriptions.
/// - `Inherited`: deadline travels in the message header — latency-
///   aware pipelines extract it per-message at dispatch time.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DeadlinePolicy {
    Released,
    #[default]
    Activated,
    Inherited,
}

/// RFC-0052 / phase-296 W3b.5 — what the executor DOES when a dispatched
/// callback runs past its bound SC's `deadline_us`. Distinct from
/// [`DeadlinePolicy`] (which says where the deadline COMES from); this is
/// the miss REACTION, lowered from the tier table's `deadline_policy`
/// string (`ignore | warn | skip | fault`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DeadlineAction {
    /// Measure nothing, report nothing (uncontracted default).
    #[default]
    Ignore,
    /// Push a `deadline-miss-runtime` violation onto the monitor drain.
    Warn,
    /// Warn AND skip the offending SC's remaining callbacks for the rest
    /// of this spin cycle (damage containment: a runaway callback does
    /// not get to also starve unrelated groups with its siblings).
    Skip,
    /// Warn AND invoke the executor's fault hook (panic when none is
    /// registered — on embedded targets that is a watchdog-visible stop).
    Fault,
}

impl DeadlineAction {
    /// Lower the tier-table string (`[tiers.<t>].deadline_policy`).
    /// Unknown strings map to `Ignore` — the bake already validated the
    /// vocabulary; runtime tolerance here avoids a boot-time panic path.
    pub fn from_tier_str(s: &str) -> Self {
        match s {
            "warn" => Self::Warn,
            "skip" => Self::Skip,
            "fault" => Self::Fault,
            _ => Self::Ignore,
        }
    }
}

/// Identifier for a [`SchedContext`] registered with an Executor.
/// 110.B.b adds storage `[Option<SchedContext>; MAX_SC]`; this index
/// addresses into that array.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SchedContextId(pub u8);

/// First-class scheduling capability — one SC per scheduling concern,
/// shared by every callback that should run under the same budget /
/// period / deadline / class.
///
/// Phase 110.B.a defines the shape; 110.B.b's builder methods on
/// Executor consume it.
#[derive(Debug, Clone, Copy, Default)]
pub struct SchedContext {
    pub class: SchedClass,
    pub priority: Priority,
    pub period_us: OptUs,
    pub budget_us: OptUs,
    pub deadline_us: OptUs,
    pub deadline_policy: DeadlinePolicy,
    /// Phase 110.F — opt-in OS-level priority for per-callback
    /// dispatch. `0` (default) means "no per-callback OS priority"
    /// — the executor's cooperative dispatch path runs every
    /// callback bound to this SC. Non-zero values trigger the
    /// per-priority worker-pool path (registered via
    /// `Executor::register_os_priority_dispatcher`); each callback
    /// then runs on a worker thread the OS scheduler has elevated
    /// to that numeric priority.
    ///
    /// Numeric meaning is platform-defined (POSIX 1..99 for
    /// SCHED_FIFO; FreeRTOS 0..configMAX_PRIORITIES-1; Zephyr
    /// direction-flipped). Chain-priority assignment + chain
    /// grouping happen at the orchestration layer and are out of
    /// executor scope.
    pub os_pri: u8,
    /// Phase 110.G — time-triggered window offset within the
    /// executor's major frame. `None` (sentinel `0`) = always
    /// eligible (no TT gate); `Some(off)` + `tt_window_duration_us`
    /// gates dispatch to the half-open interval
    /// `[off, off + duration) mod major_frame`.
    ///
    /// Independent of `class` — a `Sporadic`-class SC can also be TT-
    /// gated; both gates apply (skip dispatch when EITHER fails).
    /// Pairs with `Executor::register_time_triggered_dispatcher`
    /// which sets the major-frame length.
    /// W3b.5 — reaction on a deadline miss (see [`DeadlineAction`]).
    pub deadline_action: DeadlineAction,
    pub tt_window_offset_us: OptUs,
    /// Phase 110.G — time-triggered window length. See
    /// `tt_window_offset_us`.
    pub tt_window_duration_us: OptUs,
}

/// Phase 110.E.b — atomic sporadic-server state for ISR-driven
/// refill. ISR / timer-thread context calls `refill_thunk` to top up
/// the budget; spin_once reads atomically without any `&mut` access.
///
/// Replaces the polled-clock `SporadicState` shape on platforms with
/// a `PlatformTimer` impl. The Executor still keeps the legacy
/// `SporadicState` path active on `feature = "std"` so the
/// transition is non-breaking.
pub struct AtomicSporadicState {
    pub budget_remaining_us: portable_atomic::AtomicU32,
    /// Wraps every ~50 days at ms resolution; saturates per
    /// `tick`'s monotonic-clock contract. portable-atomic provides
    /// AtomicU32 even on RISC-V `riscv32imc` / Cortex-M0+ that lack
    /// native 32-bit atomics.
    pub last_refill_ms: portable_atomic::AtomicU32,
    pub budget_capacity_us: u32,
    pub period_us: u32,
    /// Phase 110.E.b — cumulative count of dispatched callbacks
    /// whose measured wall-clock runtime exceeded the SC's
    /// `budget_us`. Bumped by the per-callback runtime closure
    /// inside `Executor::spin_once` (std-only — the no_std fallback
    /// continues to use the polled `SporadicState` path without
    /// per-callback overrun accounting). Cooperative single-thread
    /// dispatch can't preempt a runaway callback, so this counter
    /// is the diagnostic signal — the design's oneshot-IRQ-and-
    /// cancel pattern is structurally equivalent for non-preemptive
    /// callbacks, and `last_overrun_us` carries the worst-case
    /// observation for tuning. Both reset by `clear_overrun_stats`.
    pub overrun_count: portable_atomic::AtomicU32,
    /// Phase 110.E.b — most recent dispatch's overrun amount
    /// (`measured_us - budget_us`). `0` when no overrun has been
    /// observed since the last `clear_overrun_stats`. Used by
    /// monitoring code that wants to size the budget against
    /// worst-case observed runtime.
    pub last_overrun_us: portable_atomic::AtomicU32,
}

impl AtomicSporadicState {
    pub const fn new(budget_us: u32, period_us: u32) -> Self {
        Self {
            budget_remaining_us: portable_atomic::AtomicU32::new(budget_us),
            last_refill_ms: portable_atomic::AtomicU32::new(0),
            budget_capacity_us: budget_us,
            period_us,
            overrun_count: portable_atomic::AtomicU32::new(0),
            last_overrun_us: portable_atomic::AtomicU32::new(0),
        }
    }

    /// Record one overrun: callback measured runtime exceeded the
    /// SC's `budget_us`. Bumps `overrun_count` + stores the absolute
    /// overrun amount in `last_overrun_us`. Called from the
    /// per-callback runtime closure inside `Executor::spin_once`.
    #[inline]
    pub fn record_overrun(&self, overrun_us: u32) {
        self.overrun_count
            .fetch_add(1, portable_atomic::Ordering::Relaxed);
        self.last_overrun_us
            .store(overrun_us, portable_atomic::Ordering::Relaxed);
    }

    /// Reset both overrun statistics. Useful when tuning the budget
    /// across windows (monitoring code logs + clears periodically).
    #[inline]
    pub fn clear_overrun_stats(&self) {
        self.overrun_count
            .store(0, portable_atomic::Ordering::Relaxed);
        self.last_overrun_us
            .store(0, portable_atomic::Ordering::Relaxed);
    }

    /// Read the budget atomically; spin_once consults this to decide
    /// whether to skip the SC's entries this cycle.
    pub fn has_budget(&self) -> bool {
        self.budget_remaining_us
            .load(portable_atomic::Ordering::Acquire)
            > 0
    }

    /// Saturating subtract — used by spin_once after dispatching a
    /// callback bound to this SC.
    pub fn consume(&self, us: u32) {
        let mut cur = self
            .budget_remaining_us
            .load(portable_atomic::Ordering::Acquire);
        loop {
            let next = cur.saturating_sub(us);
            match self.budget_remaining_us.compare_exchange_weak(
                cur,
                next,
                portable_atomic::Ordering::Release,
                portable_atomic::Ordering::Acquire,
            ) {
                Ok(_) => return,
                Err(observed) => cur = observed,
            }
        }
    }
}

/// C-callable refill thunk that `PlatformTimer::create_periodic`
/// invokes from the platform's timer context. Single atomic store —
/// safe in any thread / ISR context.
///
/// # Safety
/// `user_data` must point at a live `AtomicSporadicState`; the caller
/// of `PlatformTimer::create_periodic` owns the lifetime contract.
pub extern "C" fn atomic_sporadic_refill_thunk(user_data: *mut core::ffi::c_void) {
    if user_data.is_null() {
        return;
    }
    let state = unsafe { &*(user_data as *const AtomicSporadicState) };
    state
        .budget_remaining_us
        .store(state.budget_capacity_us, portable_atomic::Ordering::Release);
}

/// Phase 110.E — user-space sporadic-server runtime state.
///
/// Tracks remaining `budget_us` for the current period and the wall-
/// clock instant of the last refill. The executor consults this state
/// during dispatch: when `budget_remaining_us` reaches 0 the SC is
/// suppressed until the next period boundary, at which point a refill
/// resets the counter.
///
/// Refill cadence is polled — each `spin_once` checks whether the
/// elapsed time since the last refill exceeds `period_us` and tops
/// the budget back up. Less precise than an ISR-driven refill (Phase
/// 110.E's per-platform timer hook is what gets that) but correct as
/// an upper-bound bandwidth limiter.
#[derive(Debug, Clone, Copy)]
pub struct SporadicState {
    pub budget_remaining_us: u32,
    pub budget_capacity_us: u32,
    pub period_us: u32,
    pub last_refill_ms: u64,
}

impl SporadicState {
    pub const fn new(budget_us: u32, period_us: u32) -> Self {
        Self {
            budget_remaining_us: budget_us,
            budget_capacity_us: budget_us,
            period_us,
            last_refill_ms: 0,
        }
    }

    /// Apply elapsed-time accounting since the previous spin. Returns
    /// `true` if the SC has remaining budget after the refill check.
    pub fn tick(&mut self, now_ms: u64, delta_us: u32) -> bool {
        // Refill at period boundaries — coarse but correct.
        if now_ms.saturating_sub(self.last_refill_ms) >= self.period_us as u64 / 1000 {
            self.budget_remaining_us = self.budget_capacity_us;
            self.last_refill_ms = now_ms;
        }
        self.budget_remaining_us = self.budget_remaining_us.saturating_sub(delta_us);
        self.budget_remaining_us > 0
    }
}

impl SchedContext {
    pub const fn new_fifo() -> Self {
        Self {
            class: SchedClass::Fifo,
            priority: Priority::Normal,
            period_us: OptUs::NONE,
            budget_us: OptUs::NONE,
            deadline_us: OptUs::NONE,
            deadline_policy: DeadlinePolicy::Activated,
            os_pri: 0,
            deadline_action: DeadlineAction::Ignore,
            tt_window_offset_us: OptUs::NONE,
            tt_window_duration_us: OptUs::NONE,
        }
    }
}

// ----------------------------------------------------------------------
// Phase 110.G — TimeTriggered schedule-table API.
//
// ARINC-653-style cyclic executive: the major frame is partitioned
// into fixed windows; each callback is bound to a window via
// `SchedContext { tt_window_offset_us, tt_window_duration_us }`.
// The runtime gate inside `Executor::spin_once` already enforces
// per-window dispatch suppression (Phase 110.G runtime, landed
// pre-session). This block adds the schedule-table types +
// builder helpers so callers can declare a complete cyclic
// schedule with a single API call instead of stitching
// `create_sched_context` + `bind_handle_to_sched_context` together.
// ----------------------------------------------------------------------

/// One slot in a time-triggered schedule.
///
/// Window `[offset_us, offset_us + duration_us)` within the major
/// frame. `name` is a static-lifetime label for diagnostics
/// (logging, panic messages); the runtime never inspects it.
#[derive(Debug, Clone, Copy)]
pub struct TimeTriggeredWindow {
    pub offset_us: u32,
    pub duration_us: u32,
    pub name: &'static str,
}

impl TimeTriggeredWindow {
    pub const fn new(offset_us: u32, duration_us: u32, name: &'static str) -> Self {
        Self {
            offset_us,
            duration_us,
            name,
        }
    }
}

/// Fixed-size, no_std-friendly cyclic schedule. `N` is the
/// declared maximum window count; `window_count` is the active
/// length (callers can build the array up to `N` and set
/// `window_count` to the actual size used).
#[derive(Debug)]
pub struct TimeTriggeredSchedule<const N: usize> {
    pub major_frame_us: u32,
    pub windows: [TimeTriggeredWindow; N],
    pub window_count: usize,
}

impl<const N: usize> TimeTriggeredSchedule<N> {
    /// Construct a schedule from an exhaustive `[TimeTriggeredWindow; N]`
    /// array; `window_count` is set to `N`.
    pub const fn new_full(major_frame_us: u32, windows: [TimeTriggeredWindow; N]) -> Self {
        Self {
            major_frame_us,
            windows,
            window_count: N,
        }
    }

    /// Validate the schedule: every window must fit inside
    /// `[0, major_frame_us)` and windows must be non-overlapping
    /// in offset-sorted order. Sliding-window check; O(N²) is fine
    /// because TT schedules are small (rarely > 16 slots).
    pub fn validate(&self) -> Result<(), TimeTriggeredScheduleError> {
        if self.major_frame_us == 0 {
            return Err(TimeTriggeredScheduleError::ZeroMajorFrame);
        }
        if self.window_count > N {
            return Err(TimeTriggeredScheduleError::WindowCountOverflow);
        }
        for (i, w) in self.windows[..self.window_count].iter().enumerate() {
            if w.duration_us == 0 {
                return Err(TimeTriggeredScheduleError::ZeroWindowDuration { window: i });
            }
            let end = (w.offset_us as u64) + (w.duration_us as u64);
            if end > self.major_frame_us as u64 {
                return Err(TimeTriggeredScheduleError::WindowExceedsMajorFrame { window: i });
            }
            for (j, other) in self.windows[..self.window_count].iter().enumerate() {
                if i == j {
                    continue;
                }
                let o_end = (other.offset_us as u64) + (other.duration_us as u64);
                let overlaps = (w.offset_us as u64) < o_end && (other.offset_us as u64) < end;
                if overlaps {
                    return Err(TimeTriggeredScheduleError::WindowsOverlap {
                        window_a: i,
                        window_b: j,
                    });
                }
            }
        }
        Ok(())
    }
}

/// Validation errors for a [`TimeTriggeredSchedule`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeTriggeredScheduleError {
    ZeroMajorFrame,
    WindowCountOverflow,
    ZeroWindowDuration { window: usize },
    WindowExceedsMajorFrame { window: usize },
    WindowsOverlap { window_a: usize, window_b: usize },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opt_us_sentinel_round_trip() {
        assert!(!OptUs::NONE.is_some());
        assert_eq!(OptUs::NONE.get(), None);
        let some = OptUs::from_us(42);
        assert!(some.is_some());
        assert_eq!(some.get().map(|nz| nz.get()), Some(42));
        assert_eq!(some.raw(), 42);
    }

    #[test]
    fn opt_us_layout_is_u32() {
        // ABI guard — `OptUs` MUST stay `#[repr(transparent)]` over
        // `u32` so cbindgen emits a plain `uint32_t`.
        assert_eq!(core::mem::size_of::<OptUs>(), core::mem::size_of::<u32>());
        assert_eq!(core::mem::align_of::<OptUs>(), core::mem::align_of::<u32>());
    }

    #[test]
    fn sched_context_default_is_fifo() {
        let sc = SchedContext::default();
        assert_eq!(sc.class, SchedClass::Fifo);
        assert!(!sc.period_us.is_some());
        assert!(!sc.budget_us.is_some());
        assert!(!sc.deadline_us.is_some());
        assert_eq!(sc.deadline_policy, DeadlinePolicy::Activated);
    }
}
