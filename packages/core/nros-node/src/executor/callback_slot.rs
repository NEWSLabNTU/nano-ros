//! What a census and a profile call one executor callback slot.
//!
//! Timers, guard conditions and devices never reach the RMW, so the executor
//! reports each from its registration funnel (`registration_observer`). The
//! kind and the slot's sequence number are named here, outside that
//! feature-gated module, because a profile build labels the slot by them too.

// Issues 2984 and 2985.
/// Which kind of executor callback slot a registration claimed, as the census
/// tells them apart (`nros::node_metadata::TimerKind`, which this crate sits
/// below).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallbackSlotKind {
    /// A repeating timer on the steady clock.
    Wall,
    /// A repeating timer on a chosen clock.
    Clock,
    /// A timer that fires once.
    Oneshot,
    /// A repeating timer in a named callback group.
    InGroup,
    /// A guard condition: one slot, no period.
    GuardCondition,
    /// A device event source: one slot, no period.
    Device,
}

impl CallbackSlotKind {
    /// The prefix of the slot's synthetic id (`timer<n>`, `guard<n>`,
    /// `device<n>`), which the census and the profile both name it by.
    pub const fn id_prefix(self) -> &'static str {
        match self {
            Self::GuardCondition => "guard",
            Self::Device => "device",
            Self::Wall | Self::Clock | Self::Oneshot | Self::InGroup => "timer",
        }
    }
}

/// The next callback slot's sequence number: one sequence for every slot
/// kind, shared by the census and the profile, so both name a slot alike.
///
/// Its only callers are the executor's registration funnels, which exist only
/// with an RMW seam.
#[cfg(all(
    any(feature = "registration-observer", feature = "profile-mode"),
    any(has_rmw, test)
))]
pub(crate) fn next_slot_seq() -> usize {
    use core::sync::atomic::{AtomicUsize, Ordering};
    static SEQ: AtomicUsize = AtomicUsize::new(0);
    SEQ.fetch_add(1, Ordering::Relaxed)
}
