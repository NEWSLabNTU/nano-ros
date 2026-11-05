//! phase-521 W8 (a) (RFC-0113 D9) — the PX4 work queues a tier maps onto.
//!
//! PX4 owns `main` and every thread, so a PX4 image's sched tasks do not get threads
//! of their own: each sched task is one `px4::ScheduledWorkItem` on one of PX4's
//! work queues, and the queue's thread carries the priority
//! (`px4::wq_configurations`). This module is the table those queues come
//! from and the rule that picks one per tier. The generated entry (W8 b) asks
//! it; nothing else may spell the queue names.
//!
//! The table is RANK-ORDERED, most urgent first. The most urgent sched task gets
//! `rate_ctrl`, the next `hp_default`, then `nav_and_controllers`, then
//! `lp_default`. An image with more tiers than queues is REFUSED, never folded
//! onto a shared queue: two tiers on one queue run one after the other on one
//! thread, so the less urgent one's work would delay the more urgent one's and
//! the priority the image declared would be a fiction.

use core::fmt;

/// PX4's work queues a sched task may take, most urgent first
/// (`platforms/common/include/px4_platform_common/px4_work_queue/WorkQueueManager.hpp`,
/// `wq_configurations`).
pub const PX4_WORK_QUEUES: &[&str] = &[
    "rate_ctrl",
    "hp_default",
    "nav_and_controllers",
    "lp_default",
];

/// An image declared more sched tasks than [`PX4_WORK_QUEUES`] has queues.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Px4TooManySchedTasks {
    /// Every sched task the image declared, most urgent first.
    pub tiers: Vec<String>,
}

impl fmt::Display for Px4TooManySchedTasks {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "a PX4 image maps each tier onto its own work queue, and PX4 has {} ({}); this \
             image declares {} tiers ({}). Merge tiers until there are at most {}: two tiers \
             on one queue would share one thread, so the less urgent one would delay the \
             more urgent one (RFC-0113 D9)",
            PX4_WORK_QUEUES.len(),
            PX4_WORK_QUEUES.join(", "),
            self.tiers.len(),
            self.tiers.join(", "),
            PX4_WORK_QUEUES.len(),
        )
    }
}

impl std::error::Error for Px4TooManySchedTasks {}

/// The work queue for each sched task. `sched_tasks` is the image's sched tasks, MOST URGENT
/// FIRST (the caller orders them; the IR's tier tables already carry the
/// rank). The k-th most urgent sched task gets the k-th queue of
/// [`PX4_WORK_QUEUES`].
///
/// Refuses when there are more tiers than queues; an image with no tiers gets
/// no queues.
pub fn px4_work_queues<S: AsRef<str>>(
    tiers: &[S],
) -> Result<Vec<(&str, &'static str)>, Px4TooManySchedTasks> {
    if tiers.len() > PX4_WORK_QUEUES.len() {
        return Err(Px4TooManySchedTasks {
            tiers: tiers.iter().map(|t| t.as_ref().to_string()).collect(),
        });
    }
    Ok(tiers
        .iter()
        .zip(PX4_WORK_QUEUES)
        .map(|(t, q)| (t.as_ref(), *q))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_most_urgent_sched_task_takes_rate_ctrl_and_the_rest_follow_in_rank() {
        let got = px4_work_queues(&["control", "estimate", "plan", "log"]).unwrap();
        assert_eq!(
            got,
            vec![
                ("control", "rate_ctrl"),
                ("estimate", "hp_default"),
                ("plan", "nav_and_controllers"),
                ("log", "lp_default"),
            ]
        );
    }

    #[test]
    fn fewer_sched_tasks_take_the_most_urgent_queues() {
        let got = px4_work_queues(&["only"]).unwrap();
        assert_eq!(got, vec![("only", "rate_ctrl")]);
        let none: [&str; 0] = [];
        assert!(px4_work_queues(&none).unwrap().is_empty());
    }

    #[test]
    fn more_sched_tasks_than_queues_is_refused_naming_every_sched_task() {
        let tiers = ["a", "b", "c", "d", "e"];
        let err = px4_work_queues(&tiers).unwrap_err();
        assert_eq!(err.tiers, ["a", "b", "c", "d", "e"]);
        let msg = err.to_string();
        assert!(msg.contains("declares 5 tiers (a, b, c, d, e)"), "{msg}");
        assert!(
            msg.contains("rate_ctrl, hp_default, nav_and_controllers, lp_default"),
            "{msg}"
        );
        assert!(msg.contains("RFC-0113 D9"), "{msg}");
    }

    #[test]
    fn the_table_is_four_distinct_queues_most_urgent_first() {
        assert_eq!(PX4_WORK_QUEUES.len(), 4);
        assert_eq!(PX4_WORK_QUEUES[0], "rate_ctrl");
        assert_eq!(PX4_WORK_QUEUES[3], "lp_default");
        let mut v = PX4_WORK_QUEUES.to_vec();
        v.sort();
        v.dedup();
        assert_eq!(v.len(), PX4_WORK_QUEUES.len());
    }

    #[test]
    fn px4_is_a_known_framework_without_a_rust_entry() {
        assert!(crate::is_known_framework("px4"));
        assert!(crate::cpp_only_framework_reason("px4").is_some());
        assert!(crate::cpp_only_framework_reason("rtic").is_none());
    }
}
