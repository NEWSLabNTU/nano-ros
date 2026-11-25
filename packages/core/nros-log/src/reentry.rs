// Issue 2762.
//! The bound on direct deliveries to re-entrant sinks.
//!
//! A re-entrant sink gets each record directly, in the context that raised it.
//! The bound exists for ONE case: a re-entrant sink that logs through
//! `nros_log` from inside its own `log()`, which would otherwise recurse
//! without limit. So it counts the nesting of ONE task. Concurrent loggers on
//! different tasks are not recursion and must never spend each other's bound:
//! when they did (one process-wide counter), sixteen loggers on a two-CPU
//! Zephyr image lost about two thirds of their records, because past eight
//! deliveries in flight every record took the guarded path and overflowed the
//! hold queue.
//!
//! With the port's task identity (`cfg(nros_log_task_identity)`,
//! `nros_platform_current_task_id`), each task inside a direct delivery holds
//! one slot of [`SLOTS`], with its own nesting depth. Without it, or with
//! every slot taken, the process-wide counter answers as before: correct, and
//! only conservative.
//!
//! An interrupt reads as the task it interrupted (or as "unknown"), so an
//! interrupt's record nested inside that task's delivery counts against that
//! task's depth. That is what bounds it, and on one CPU the task cannot run
//! meanwhile, so the two never race on the slot.

use core::sync::atomic::{AtomicUsize, Ordering};

use crate::REENTRANT_DEPTH;

/// Re-entrant deliveries in progress, process-wide: the bound when the task
/// is unknown (issue 2762's first form, kept as the fallback).
static GLOBAL_ACTIVE: AtomicUsize = AtomicUsize::new(0);

/// How one direct delivery was admitted, so [`leave`] undoes exactly that.
#[derive(Clone, Copy)]
pub(crate) enum Entered {
    /// Counted in the process-wide counter.
    Global,
    /// Counted in this task's slot.
    #[cfg_attr(not(any(nros_log_task_identity, test)), allow(dead_code))]
    Task(usize),
}

/// Admit one direct delivery for the calling context, or `None` when its
/// depth is spent (the record then takes the guarded path for every sink).
pub(crate) fn enter() -> Option<Entered> {
    #[cfg(nros_log_task_identity)]
    if let Some(admitted) = task::enter(task::current()) {
        return admitted;
    }
    if GLOBAL_ACTIVE.fetch_add(1, Ordering::AcqRel) < REENTRANT_DEPTH {
        Some(Entered::Global)
    } else {
        GLOBAL_ACTIVE.fetch_sub(1, Ordering::AcqRel);
        None
    }
}

/// End a delivery [`enter`] admitted.
pub(crate) fn leave(entered: Entered) {
    match entered {
        Entered::Global => {
            GLOBAL_ACTIVE.fetch_sub(1, Ordering::AcqRel);
        }
        #[cfg(any(nros_log_task_identity, test))]
        Entered::Task(slot) => task::leave(slot),
        #[cfg(not(any(nros_log_task_identity, test)))]
        Entered::Task(_) => {}
    }
}

/// What [`spend_all`] took, for [`give_back`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Spent {
    Global,
    #[cfg_attr(not(any(nros_log_task_identity, test)), allow(dead_code))]
    Task {
        slot: usize,
        before: usize,
    },
}

impl Spent {
    /// One nonzero word for an atomic: `1` process-wide, else
    /// `2 + slot * (REENTRANT_DEPTH + 1) + before` (`before <= REENTRANT_DEPTH`).
    pub(crate) fn encode(self) -> usize {
        match self {
            Spent::Global => 1,
            Spent::Task { slot, before } => 2 + slot * (REENTRANT_DEPTH + 1) + before,
        }
    }

    /// [`Self::encode`]'s inverse; `0` is "nothing spent".
    pub(crate) fn decode(word: usize) -> Option<Self> {
        match word {
            0 => None,
            1 => Some(Spent::Global),
            w => {
                let w = w - 2;
                Some(Spent::Task {
                    slot: w / (REENTRANT_DEPTH + 1),
                    before: w % (REENTRANT_DEPTH + 1),
                })
            }
        }
    }
}

/// Spend the calling task's WHOLE depth, for the guard holder's delivery of a
/// HELD record: whatever a re-entrant sink logs from inside it is then held,
/// not delivered on top of it. Per task where the task is known, so other
/// tasks' direct deliveries go on meanwhile; process-wide otherwise.
pub(crate) fn spend_all() -> Spent {
    #[cfg(nros_log_task_identity)]
    if let Some(spent) = task::spend_all(task::current()) {
        return spent;
    }
    GLOBAL_ACTIVE.fetch_add(REENTRANT_DEPTH, Ordering::AcqRel);
    Spent::Global
}

/// Undo [`spend_all`].
pub(crate) fn give_back(spent: Spent) {
    match spent {
        Spent::Global => {
            GLOBAL_ACTIVE.fetch_sub(REENTRANT_DEPTH, Ordering::AcqRel);
        }
        #[cfg(any(nros_log_task_identity, test))]
        Spent::Task { slot, before } => task::restore(slot, before),
        #[cfg(not(any(nros_log_task_identity, test)))]
        Spent::Task { .. } => {}
    }
}

#[cfg(any(nros_log_task_identity, test))]
mod task {
    use core::sync::atomic::{AtomicUsize, Ordering};

    use super::{Entered, Spent};
    use crate::REENTRANT_DEPTH;

    /// Tasks that can be inside a direct delivery at once before the next
    /// one is counted process-wide instead. A slot is two words.
    pub(super) const SLOTS: usize = 64;

    /// The task holding each slot, `0` when free.
    static TASK: [AtomicUsize; SLOTS] = [const { AtomicUsize::new(0) }; SLOTS];
    /// Its nesting depth. Only that task (or an interrupt nested in it, which
    /// it cannot run beside) changes it.
    static DEPTH: [AtomicUsize; SLOTS] = [const { AtomicUsize::new(0) }; SLOTS];

    #[cfg(nros_log_task_identity)]
    pub(super) fn current() -> usize {
        unsafe extern "C" {
            fn nros_platform_current_task_id() -> usize;
        }
        // SAFETY: a plain query with no arguments; the port that declares
        // `[capabilities] task_identity` exports it (nros/platform.h).
        unsafe { nros_platform_current_task_id() }
    }

    fn find(id: usize) -> Option<usize> {
        (0..SLOTS).find(|&i| TASK[i].load(Ordering::Acquire) == id)
    }

    fn claim(id: usize, depth: usize) -> Option<usize> {
        (0..SLOTS).find(|&i| {
            TASK[i]
                .compare_exchange(0, id, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
                && {
                    DEPTH[i].store(depth, Ordering::Release);
                    true
                }
        })
    }

    /// `None` when this cannot count the task (unknown, or every slot taken):
    /// the caller falls back to the process-wide counter.
    pub(super) fn enter(id: usize) -> Option<Option<Entered>> {
        if id == 0 {
            return None;
        }
        if let Some(slot) = find(id) {
            // This task is already inside a delivery: recursion.
            if DEPTH[slot].fetch_add(1, Ordering::AcqRel) < REENTRANT_DEPTH {
                return Some(Some(Entered::Task(slot)));
            }
            DEPTH[slot].fetch_sub(1, Ordering::AcqRel);
            return Some(None);
        }
        claim(id, 1).map(|slot| Some(Entered::Task(slot)))
    }

    pub(super) fn leave(slot: usize) {
        if DEPTH[slot].fetch_sub(1, Ordering::AcqRel) == 1 {
            TASK[slot].store(0, Ordering::Release);
        }
    }

    pub(super) fn spend_all(id: usize) -> Option<Spent> {
        if id == 0 {
            return None;
        }
        if let Some(slot) = find(id) {
            let before = DEPTH[slot].swap(REENTRANT_DEPTH, Ordering::AcqRel);
            return Some(Spent::Task { slot, before });
        }
        claim(id, REENTRANT_DEPTH).map(|slot| Spent::Task { slot, before: 0 })
    }

    pub(super) fn restore(slot: usize, before: usize) {
        DEPTH[slot].store(before, Ordering::Release);
        if before == 0 {
            TASK[slot].store(0, Ordering::Release);
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use std::vec::Vec;

    use super::{
        Entered, Spent,
        task::{self, SLOTS},
    };
    use crate::REENTRANT_DEPTH;

    fn slot(e: Option<Option<Entered>>) -> usize {
        match e {
            Some(Some(Entered::Task(slot))) => slot,
            _ => panic!("not admitted on a task slot"),
        }
    }

    // One test, so the shared slot table is never used by two at once.
    #[test]
    fn the_bound_is_per_task_not_per_process() {
        // Concurrent loggers: many tasks inside at once, none refused.
        let ids: Vec<usize> = (1..=32).map(|i| 0x1000 + i).collect();
        let slots: Vec<usize> = ids.iter().map(|&id| slot(task::enter(id))).collect();
        // Recursion on ONE of them is still bounded at REENTRANT_DEPTH.
        let mut nested = Vec::new();
        for _ in 1..REENTRANT_DEPTH {
            nested.push(slot(task::enter(ids[0])));
        }
        assert!(
            matches!(task::enter(ids[0]), Some(None)),
            "the {}th nested delivery of one task must take the guarded path",
            REENTRANT_DEPTH + 1
        );
        // ...while another task still gets in.
        let other = slot(task::enter(ids[1]));
        task::leave(other);
        for s in nested {
            task::leave(s);
        }
        for s in slots {
            task::leave(s);
        }

        // Every slot taken: the next task is not counted here (the caller
        // falls back to the process-wide counter), never refused outright.
        let all: Vec<usize> = (1..=SLOTS).map(|i| slot(task::enter(0x2000 + i))).collect();
        assert!(task::enter(0x9999).is_none(), "a full table falls back");
        for s in all {
            task::leave(s);
        }
        assert!(task::enter(0).is_none(), "an unknown task falls back");

        // A held record's delivery spends the holder's whole depth, so a
        // record its sink logs is held; giving it back frees the slot.
        let Some(spent) = task::spend_all(0x3000) else {
            panic!("spend_all claims a slot")
        };
        assert!(matches!(task::enter(0x3000), Some(None)));
        assert_eq!(Spent::decode(spent.encode()), Some(spent));
        super::give_back(spent);
        let s = slot(task::enter(0x3000));
        task::leave(s);
    }

    #[test]
    fn spent_encodes_into_one_nonzero_word() {
        assert_eq!(Spent::decode(0), None);
        for spent in [
            Spent::Global,
            Spent::Task { slot: 0, before: 0 },
            Spent::Task {
                slot: SLOTS - 1,
                before: REENTRANT_DEPTH,
            },
        ] {
            assert_ne!(spent.encode(), 0);
            assert_eq!(Spent::decode(spent.encode()), Some(spent));
        }
    }
}
