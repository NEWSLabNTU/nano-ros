//! The deadlines a run-to-completion level runner wakes its
//! levels for (`levels::RunToCompletionLevels`), kept apart from it because
//! this half is pure arithmetic over atomics and needs no executor or platform
//! port, so its tests run in the lane that links none.

use core::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering};

/// Each level's next timer deadline, as the low 32 bits of a microsecond
/// clock, compared by wrapping difference: 32-bit atomics, since a Cortex-M3
/// has no 64-bit ones.
pub struct LevelWakeTimes {
    wake_at: [AtomicU32; crate::MAX_LEVELS],
    armed: [AtomicBool; crate::MAX_LEVELS],
    boot: AtomicU8,
}

impl LevelWakeTimes {
    /// The furthest a deadline is armed ahead: half the 32-bit microsecond
    /// range (~35 min), so a wrapping difference stays unambiguous. A longer
    /// one wakes the level early and it re-arms the rest.
    pub const HORIZON_US: u32 = i32::MAX as u32;

    /// No level armed, boot level 0.
    pub const fn new() -> Self {
        Self {
            wake_at: [const { AtomicU32::new(0) }; crate::MAX_LEVELS],
            armed: [const { AtomicBool::new(false) }; crate::MAX_LEVELS],
            boot: AtomicU8::new(0),
        }
    }

    /// Which level boots. Its deadline never arms the wake: its own timers
    /// bound its own park.
    pub fn set_boot(&self, level: u8) {
        self.boot.store(level, Ordering::Relaxed);
    }

    /// Record `level`'s next deadline, `after_us` microseconds after `now`
    /// (`None`: its band arms no timer).
    pub fn set(&self, level: u8, after_us: Option<u64>, now: u32) {
        let i = usize::from(level);
        if i >= crate::MAX_LEVELS {
            return;
        }
        match after_us {
            Some(d) => {
                let d = d.min(u64::from(Self::HORIZON_US)) as u32;
                self.wake_at[i].store(now.wrapping_add(d), Ordering::Release);
                self.armed[i].store(true, Ordering::Release);
            }
            None => self.armed[i].store(false, Ordering::Release),
        }
    }

    /// Microseconds from `now` to the earliest armed non-boot deadline (0 when
    /// one is already due); `None` when none is armed.
    pub fn soonest_after(&self, now: u32) -> Option<u32> {
        let boot = usize::from(self.boot.load(Ordering::Relaxed));
        (0..crate::MAX_LEVELS)
            .filter(|i| *i != boot && self.armed[*i].load(Ordering::Acquire))
            .map(|i| remaining(self.wake_at[i].load(Ordering::Acquire), now))
            .min()
    }

    /// Whether `level`'s deadline has passed at `now`.
    pub fn is_due(&self, level: u8, now: u32) -> bool {
        let i = usize::from(level);
        i < crate::MAX_LEVELS
            && self.armed[i].load(Ordering::Acquire)
            && remaining(self.wake_at[i].load(Ordering::Acquire), now) == 0
    }
}

impl Default for LevelWakeTimes {
    fn default() -> Self {
        Self::new()
    }
}

/// Microseconds from `now` to `at`, 0 once `at` has passed (both the low 32
/// bits of one clock).
fn remaining(at: u32, now: u32) -> u32 {
    let d = at.wrapping_sub(now) as i32;
    if d > 0 { d as u32 } else { 0 }
}

#[cfg(test)]
mod tests {
    use super::LevelWakeTimes;

    /// The board one-shot is armed at the earliest NON-boot deadline: the boot
    /// level's own timers bound its own park, so its deadline never arms it.
    #[test]
    fn the_wake_is_armed_at_the_earliest_non_boot_deadline() {
        let t = LevelWakeTimes::new();
        t.set_boot(2);
        assert_eq!(t.soonest_after(1_000), None, "nothing armed: disarm");
        t.set(2, Some(10), 1_000);
        assert_eq!(
            t.soonest_after(1_000),
            None,
            "the boot level's deadline is not the wake's"
        );
        t.set(0, Some(500), 1_000);
        t.set(1, Some(200), 1_000);
        assert_eq!(t.soonest_after(1_000), Some(200));
        assert_eq!(t.soonest_after(1_300), Some(0), "already due fires at once");
        t.set(1, None, 1_300);
        assert_eq!(
            t.soonest_after(1_300),
            Some(200),
            "a band with no timer leaves the wake"
        );
    }

    /// Due means the deadline has passed by the clock's own wrapping
    /// difference, so a deadline just past a 32-bit wrap is still ahead and
    /// one just before it is still due.
    #[test]
    fn due_survives_the_32_bit_wrap() {
        let t = LevelWakeTimes::new();
        t.set_boot(1);
        t.set(0, Some(16), u32::MAX - 10);
        assert!(!t.is_due(0, u32::MAX - 10), "16 us ahead, across the wrap");
        assert_eq!(t.soonest_after(u32::MAX - 10), Some(16));
        assert!(t.is_due(0, 6), "past the deadline after the wrap");
        assert!(!t.is_due(1, 6), "an unarmed level is never due");
    }

    /// A deadline past the horizon is armed AT the horizon, so the wrapping
    /// difference never reads it as passed.
    #[test]
    fn a_deadline_past_the_horizon_is_clamped_not_wrapped() {
        let t = LevelWakeTimes::new();
        t.set_boot(1);
        t.set(0, Some(u64::from(u32::MAX) * 4), 0);
        assert_eq!(t.soonest_after(0), Some(LevelWakeTimes::HORIZON_US));
        assert!(!t.is_due(0, 0));
    }
}
