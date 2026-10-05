//! RFC-0102 D1 — `create_child` reports an exhausted arena as `ArenaFull`,
//! never as a logger under the wrong name. Its own process: it spends every
//! runtime-logger slot.

use nros_log::{ChildError, dynamic_logger_capacity, get_or_create_logger};

#[test]
fn an_exhausted_arena_is_arena_full() {
    let parent = get_or_create_logger("full").expect("first slot");
    let mut i = 0usize;
    while get_or_create_logger(&format!("f{i}")).is_some() {
        i += 1;
        assert!(i <= dynamic_logger_capacity(), "the arena never filled");
    }
    assert_eq!(parent.create_child("c").err(), Some(ChildError::ArenaFull));
}
