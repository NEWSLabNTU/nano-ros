//! Phase 110.B — `SchedContext` API + supporting types.
//!
//! A `SchedContext` is a first-class scheduling capability. Multiple
//! callbacks share one SC; one OS priority slot per Executor regardless
//! of callback count. Inspired by seL4 MCS (Mixed-Criticality
//! Scheduling).
//!
//! 110.B.a (this commit) lands the type surface + `EdfReadySet`. The
//! Executor builder methods (`create_sched_context`,
//! `add_subscription_in`, ...) and the cbindgen / C / C++ wrappers
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
#[allow(dead_code)] // Phase 110.B.a — wired in 110.B.b builder/dispatch.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(transparent)]
pub struct OptUs(u32);

#[allow(dead_code)] // Phase 110.B.a — wired in 110.B.b builder/dispatch.
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
#[allow(dead_code)] // Phase 110.B.a — wired in 110.B.b builder/dispatch.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SchedClass {
    #[default]
    Fifo,
    Edf,
    Sporadic,
    BestEffort,
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
#[allow(dead_code)] // Phase 110.C — wired in spin_once bucketed dispatch.
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

#[allow(dead_code)] // Phase 110.C — wired in spin_once bucketed dispatch.
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
#[allow(dead_code)] // Phase 110.B.a — wired in 110.B.b builder/dispatch.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DeadlinePolicy {
    Released,
    #[default]
    Activated,
    Inherited,
}

/// Identifier for a [`SchedContext`] registered with an Executor.
/// 110.B.b adds storage `[Option<SchedContext>; MAX_SC]`; this index
/// addresses into that array.
#[allow(dead_code)] // Phase 110.B.a — wired in 110.B.b builder/dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SchedContextId(pub u8);

/// First-class scheduling capability — one SC per scheduling concern,
/// shared by every callback that should run under the same budget /
/// period / deadline / class.
///
/// Phase 110.B.a defines the shape; 110.B.b's builder methods on
/// Executor consume it.
#[allow(dead_code)] // Phase 110.B.a — wired in 110.B.b builder/dispatch.
#[derive(Debug, Clone, Copy, Default)]
pub struct SchedContext {
    pub class: SchedClass,
    pub priority: Priority,
    pub period_us: OptUs,
    pub budget_us: OptUs,
    pub deadline_us: OptUs,
    pub deadline_policy: DeadlinePolicy,
}

#[allow(dead_code)] // Phase 110.B.a — wired in 110.B.b builder/dispatch.
impl SchedContext {
    pub const fn new_fifo() -> Self {
        Self {
            class: SchedClass::Fifo,
            priority: Priority::Normal,
            period_us: OptUs::NONE,
            budget_us: OptUs::NONE,
            deadline_us: OptUs::NONE,
            deadline_policy: DeadlinePolicy::Activated,
        }
    }
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
        assert_eq!(
            core::mem::align_of::<OptUs>(),
            core::mem::align_of::<u32>()
        );
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
