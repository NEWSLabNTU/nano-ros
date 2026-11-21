//! What a sched task's THREAD is called, for every runner that spawns one.
//!
//! A kernel keeps few bytes of a thread name: Linux 15 plus the NUL, and
//! FreeRTOS's default `configMAX_TASK_NAME_LEN` is 16. The POSIX port cuts a
//! longer name there (glibc refuses it outright), so whatever tells two
//! threads apart has to come first. A runner names a sched task's thread
//! `nros-<label>`, with the label from [`label`](crate::thread_name::label).

/// The prefix the priority derivation gives every sched task it synthesises
/// (`derived-<node>`, `derived-level-<k>`). Every derived level carries it, so
/// in a thread name it is eight bytes that tell no two levels apart.
pub const DERIVED_PREFIX: &str = "derived-";

/// The part of a sched task's name its thread is labelled with: the name, less
/// [`DERIVED_PREFIX`]. A runner formats it as `nros-<label>`.
pub fn label(sched_task: &str) -> &str {
    sched_task
        .strip_prefix(DERIVED_PREFIX)
        .unwrap_or(sched_task)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_derived_level_loses_the_prefix() {
        assert_eq!(label("derived-urgent"), "urgent");
        assert_eq!(label("derived-level-2"), "level-2");
    }

    #[test]
    fn an_authored_sched_task_keeps_its_name() {
        assert_eq!(label("high"), "high");
        assert_eq!(label("derived"), "derived");
    }
}
