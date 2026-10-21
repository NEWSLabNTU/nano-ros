// Copyright (c) 2026, NEWSLab NTU.
// SPDX-License-Identifier: Apache-2.0

//! phase-509 P7 (RFC-0108 D8.1, D8.2) -- which level an executor is, and
//! whether every level of the image is alive.
//!
//! A supervisor that owns a route per level, or feeds a hardware watchdog,
//! needs two facts the executor alone knows:
//!
//! - **Identity.** Each executor runs one level ([`Executor::level`]): 0, the
//!   most urgent, unless its runner set another ([`Executor::set_level`]).
//!   The generated entry names each node's level at build time
//!   (`NROS_ENTRY_LEVEL_OF_<node>` in `nros_priority_plan.h`) by the same
//!   rule the runner applies: a level's index is its tier's urgency order
//!   among the image's tiers (until phase-509 P9 a level is a tier).
//! - **Liveness.** Every spin of an executor checks its level in here, with
//!   the time ([`level_last_spin_us`]) and a spin count. The watchdog feed is
//!   [`take_every_level_alive`]: `true` only when every level of the image
//!   (`0..level_count()`) has checked in since the last `true`, which it
//!   then clears. A level blocked behind a stuck callback stops checking in,
//!   so the feed stops even while another level spins on (RFC-0108 D8.2).
//!
//! The registry is per image (a `static`): the levels are separate executors
//! on separate threads until P9, and the watchdog is per image. Lock-free,
//! 32-bit atomics only, so it works on a core without 64-bit atomics; a time
//! is the low 32 bits of the executor's microsecond clock, so a consumer
//! compares two of them with a wrapping difference (71 minutes of range).
//!
//! [`Executor::level`]: super::Executor::level
//! [`Executor::set_level`]: super::Executor::set_level

use portable_atomic::{AtomicU8, AtomicU32, Ordering::SeqCst};

/// Levels one image can have: `CONFIG_NROS_EXECUTOR_MAX_LEVELS`'s upper end,
/// one bit each in the check-in word.
pub const MAX_LEVELS: usize = 32;

/// The liveness of an image's levels.
pub(crate) struct LevelRegistry {
    count: AtomicU8,
    checked_in: AtomicU32,
    spins: [AtomicU32; MAX_LEVELS],
    last_spin_us: [AtomicU32; MAX_LEVELS],
}

impl LevelRegistry {
    pub(crate) const fn new() -> Self {
        Self {
            count: AtomicU8::new(1),
            checked_in: AtomicU32::new(0),
            spins: [const { AtomicU32::new(0) }; MAX_LEVELS],
            last_spin_us: [const { AtomicU32::new(0) }; MAX_LEVELS],
        }
    }

    fn all(&self) -> u32 {
        let n = self.count.load(SeqCst) as u32;
        if n >= 32 { u32::MAX } else { (1u32 << n) - 1 }
    }

    pub(crate) fn set_count(&self, n: u8) -> bool {
        if n == 0 || n as usize > MAX_LEVELS {
            return false;
        }
        self.count.store(n, SeqCst);
        true
    }

    pub(crate) fn count(&self) -> u8 {
        self.count.load(SeqCst)
    }

    /// Every spin of an executor calls this; a build with no executor (no
    /// RMW seam) has no caller.
    #[cfg_attr(not(any(has_rmw, test)), allow(dead_code))]
    pub(crate) fn check_in(&self, level: u8, now_us: Option<u64>) {
        let l = level as usize;
        if l >= MAX_LEVELS {
            return;
        }
        if let Some(t) = now_us {
            self.last_spin_us[l].store(t as u32, SeqCst);
        }
        self.spins[l].fetch_add(1, SeqCst);
        self.checked_in.fetch_or(1u32 << l, SeqCst);
    }

    pub(crate) fn spins(&self, level: u8) -> u32 {
        self.spins.get(level as usize).map_or(0, |s| s.load(SeqCst))
    }

    pub(crate) fn last_spin_us(&self, level: u8) -> Option<u32> {
        (self.spins(level) != 0).then(|| self.last_spin_us[level as usize].load(SeqCst))
    }

    pub(crate) fn every_alive(&self) -> bool {
        let all = self.all();
        self.checked_in.load(SeqCst) & all == all
    }

    pub(crate) fn take_every_alive(&self) -> bool {
        let all = self.all();
        loop {
            let cur = self.checked_in.load(SeqCst);
            if cur & all != all {
                return false;
            }
            if self
                .checked_in
                .compare_exchange(cur, cur & !all, SeqCst, SeqCst)
                .is_ok()
            {
                return true;
            }
        }
    }
}

pub(crate) static LEVELS: LevelRegistry = LevelRegistry::new();

/// The level of tier `index` of an image whose `n` tiers have the kernel
/// priorities `priority_of(0..n)`: its urgency order, 0 the most urgent,
/// equal priorities in array order. Until phase-509 P9 a level is a tier, so
/// this is how a runner places each tier's executor
/// ([`Executor::set_level`](super::Executor::set_level)). `nros codegen
/// entry` writes `NROS_ENTRY_LEVEL_OF_<node>` by the same rule, over the same
/// array. `None` when `index` is out of range.
pub fn tier_level_by(
    n: usize,
    index: usize,
    smaller_is_urgent: bool,
    priority_of: impl Fn(usize) -> i64,
) -> Option<u8> {
    if index >= n {
        return None;
    }
    let mine = priority_of(index);
    let ahead = (0..n)
        .filter(|&j| {
            let q = priority_of(j);
            (if smaller_is_urgent {
                q < mine
            } else {
                q > mine
            }) || (q == mine && j < index)
        })
        .count();
    u8::try_from(ahead).ok()
}

/// [`tier_level_by`] over a slice of priorities.
pub fn tier_level(priorities: &[i64], index: usize, smaller_is_urgent: bool) -> Option<u8> {
    tier_level_by(priorities.len(), index, smaller_is_urgent, |i| {
        priorities[i]
    })
}

/// State how many levels the image runs (its runner does, once, before the
/// levels spin). `false`, and nothing changed, for 0 or more than
/// [`MAX_LEVELS`]. An image that never states it has one level.
pub fn set_level_count(n: u8) -> bool {
    LEVELS.set_count(n)
}

/// How many levels the image runs.
pub fn level_count() -> u8 {
    LEVELS.count()
}

/// How many times `level` has spun.
pub fn level_spins(level: u8) -> u32 {
    LEVELS.spins(level)
}

/// The low 32 bits of the executor clock, in microseconds, at `level`'s
/// latest spin; `None` when it has not spun (or spins with no clock).
pub fn level_last_spin_us(level: u8) -> Option<u32> {
    LEVELS.last_spin_us(level)
}

/// Whether every level of the image has checked in since the last
/// [`take_every_level_alive`] that returned `true`.
pub fn every_level_alive() -> bool {
    LEVELS.every_alive()
}

/// The watchdog feed: `true` when every level has checked in since the last
/// `true`, and the check-ins are then cleared; `false`, and nothing cleared,
/// while any level has not.
pub fn take_every_level_alive() -> bool {
    LEVELS.take_every_alive()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The island's shape on Zephyr (smaller is urgent): a tier at 5 is level
    /// 0 wherever it sits in the array; equal priorities keep array order;
    /// the other direction inverts it.
    #[test]
    fn a_tiers_level_is_its_urgency_order() {
        let p = [6, 5, 8, 6];
        let levels: [Option<u8>; 4] = core::array::from_fn(|i| tier_level(&p, i, true));
        assert_eq!(levels, [Some(1), Some(0), Some(3), Some(2)]);
        let levels: [Option<u8>; 4] = core::array::from_fn(|i| tier_level(&p, i, false));
        assert_eq!(levels, [Some(1), Some(3), Some(0), Some(2)]);
        assert_eq!(tier_level(&p, 4, true), None);
        assert_eq!(tier_level(&[5], 0, true), Some(0));
    }

    /// The aggregate is false while one level has not checked in, true once
    /// every level has, and taking it starts a new round.
    #[test]
    fn every_level_must_check_in_before_the_feed() {
        let r = LevelRegistry::new();
        assert_eq!(r.count(), 1, "an image that states nothing has one level");
        assert!(!r.set_count(0) && !r.set_count(33));
        assert!(r.set_count(2));
        assert_eq!(r.last_spin_us(1), None);
        r.check_in(0, Some(1_000));
        assert!(!r.every_alive(), "level 1 has not checked in");
        assert!(!r.take_every_alive());
        r.check_in(1, Some(1_250));
        assert!(r.every_alive());
        assert!(r.take_every_alive());
        assert!(!r.every_alive(), "taken: a new round starts");
        r.check_in(0, Some(2_000));
        assert!(!r.take_every_alive(), "level 0 alone does not feed");
        assert_eq!(r.last_spin_us(0), Some(2_000));
        assert_eq!((r.spins(0), r.spins(1)), (2, 1));
        // A clockless spin still checks in and counts.
        r.check_in(1, None);
        assert_eq!(r.last_spin_us(1), Some(1_250));
        assert!(r.take_every_alive());
    }
}
