//! Where a hosted POSIX runner places zenoh-pico's transport tasks: the floor
//! of the priority plan's `reserved.transport` band
//! (`packages/boards/linux/nros-board.toml`, `[90, 99]`), above every sched task.
//!
//! ONE decision for both Linux runners, the Rust board's (`nros-board-linux`)
//! and the C/C++ one (`nros-cpp`). Each reaches zenoh-pico's
//! `zpico_set_task_config` its own way, because only the Rust board knows at
//! compile time whether the zenoh backend is linked, so the CALL is the
//! caller's and everything else is here.
//!
//! Why it is needed at all: a sched task with a priority runs `SCHED_FIFO`, and
//! a `SCHED_FIFO` thread outranks every `SCHED_OTHER` thread unconditionally.
//! A transport left on `SCHED_OTHER` is preempted by the app that publishes
//! over it, always. `zpico_set_task_config` is a process-wide default read when
//! the session opens and creates the transport tasks, so this must run BEFORE
//! the open; after it, it changes nothing.

use core::fmt;

/// The floor of the plan's `reserved.transport` band. Cross-checked against
/// the plan by `check-sched-task-priority-plan`, so the two cannot drift.
pub const TRANSPORT_BAND_FLOOR: u32 = 90;

/// Place the transport tasks at [`TRANSPORT_BAND_FLOOR`] when any sched task
/// asks for a real-time priority (`max_sched_priority > 0`); with none, every
/// thread is `SCHED_OTHER` and there is nothing to sit above.
///
/// `set_task_config(priority)` applies it and returns whether a transport was
/// there to take it (false: no zenoh backend linked, which is not a
/// degradation — such an image has no transport tasks to place). Best effort
/// like every priority on this port: without `CAP_SYS_NICE` the attribute is
/// ignored at spawn. Returns the priority placed.
pub fn place(
    max_sched_priority: i64,
    set_task_config: impl FnOnce(u32) -> bool,
    mut println: impl FnMut(fmt::Arguments<'_>),
) -> Option<u32> {
    if max_sched_priority <= 0 {
        return None;
    }
    if !set_task_config(TRANSPORT_BAND_FLOOR) {
        return None;
    }
    println(format_args!(
        "nros: transport tasks at SCHED_FIFO {TRANSPORT_BAND_FLOOR} (reserved band floor; \
         most urgent sched task is {max_sched_priority})"
    ));
    Some(TRANSPORT_BAND_FLOOR)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_real_time_sched_task_places_nothing() {
        let mut called = false;
        assert_eq!(
            place(
                0,
                |_| {
                    called = true;
                    true
                },
                |_| {}
            ),
            None
        );
        assert!(!called);
    }

    #[test]
    fn a_real_time_sched_task_places_the_band_floor() {
        let mut got = None;
        assert_eq!(
            place(
                80,
                |p| {
                    got = Some(p);
                    true
                },
                |_| {}
            ),
            Some(TRANSPORT_BAND_FLOOR)
        );
        assert_eq!(got, Some(TRANSPORT_BAND_FLOOR));
    }

    #[test]
    fn no_transport_linked_places_nothing() {
        assert_eq!(place(80, |_| false, |_| {}), None);
    }
}
