//! RFC-0102 D1/D2 — `create_child`, and an unset level resolving through the
//! nearest existing dotted ancestor (rcutils's effective-level rule).
//!
//! Integration tests, so this process has its own intern table and arena. The
//! tests run concurrently, so each uses its own name prefix and none moves the
//! process default level. They share the default 16-slot runtime-logger
//! arena: the file creates 15, so a new test here must reuse a logger or move
//! to its own file.

use nros_log::{
    ChildError, DEFAULT_LOGGER, Logger, MAX_LOGGER_NAME_LEN, Severity, get_or_create_logger,
};

fn logger(name: &str) -> &'static Logger {
    get_or_create_logger(name).expect("arena slot")
}

#[test]
fn an_unset_child_filters_at_its_parents_level() {
    let parent = logger("inh");
    parent.set_level(Severity::Debug);
    let child = parent.create_child("planner").unwrap();
    assert_eq!(child.name(), "inh.planner");
    assert!(core::ptr::eq(child.parent().unwrap(), parent));
    assert!(
        child.is_enabled(Severity::Debug),
        "parent Debug must reach an unset child"
    );
    assert_eq!(child.level(), Severity::Debug);

    parent.set_level(Severity::Error);
    assert!(
        !child.is_enabled(Severity::Warn),
        "a LATER parent change must reach it too"
    );
}

#[test]
fn a_childs_own_level_overrides_its_parent() {
    let parent = logger("own");
    parent.set_level(Severity::Debug);
    let child = parent.create_child("c").unwrap();
    child.set_level(Severity::Warn);
    assert!(!child.is_enabled(Severity::Info));
    child.unset_level();
    assert!(
        child.is_enabled(Severity::Debug),
        "unset hands it back to the parent"
    );
}

#[test]
fn an_ancestor_created_after_its_descendant_is_linked() {
    let grandchild = logger("late.a.b");
    assert!(grandchild.parent().is_none(), "no ancestor exists yet");
    let root = logger("late");
    root.set_level(Severity::Debug);
    assert!(core::ptr::eq(grandchild.parent().unwrap(), root));
    assert!(grandchild.is_enabled(Severity::Debug));
    // `late` is a STRING prefix of `latex`, not a dotted one.
    assert!(logger("latex").parent().is_none());
}

#[test]
fn a_middle_ancestor_created_later_repoints_the_grandchild() {
    let root = logger("mid");
    root.set_level(Severity::Error);
    let leaf = logger("mid.x.y");
    assert!(core::ptr::eq(leaf.parent().unwrap(), root));
    let middle = logger("mid.x");
    middle.set_level(Severity::Debug);
    assert!(
        core::ptr::eq(leaf.parent().unwrap(), middle),
        "the closer ancestor wins"
    );
    assert!(leaf.is_enabled(Severity::Debug));
    assert!(core::ptr::eq(middle.parent().unwrap(), root));
}

#[test]
fn create_child_is_idempotent() {
    let parent = logger("idem");
    let a = parent.create_child("k").unwrap();
    let b = parent.create_child(String::from("k")).unwrap();
    assert!(core::ptr::eq(a, b), "same name, same logger");
}

#[test]
fn the_catch_alls_children_are_top_level_names() {
    let child = DEFAULT_LOGGER.create_child("toplevel_k").unwrap();
    assert_eq!(child.name(), "toplevel_k", "not `nros.toplevel_k`");
    assert_eq!(
        DEFAULT_LOGGER.create_child("").err(),
        Some(ChildError::EmptyName)
    );
}

#[test]
fn a_name_over_the_limit_is_refused_with_its_length() {
    let parent = logger("long");
    let suffix = "x".repeat(MAX_LOGGER_NAME_LEN - "long.".len() + 1);
    assert_eq!(
        parent.create_child(suffix.as_str()).err(),
        Some(ChildError::NameTooLong {
            len: MAX_LOGGER_NAME_LEN + 1,
            max: MAX_LOGGER_NAME_LEN
        })
    );
}

#[test]
fn a_static_parent_nobody_registered_still_passes_its_level_on() {
    static PARENT: Logger = Logger::with_level("staticparent", Severity::Debug);
    let child = PARENT.create_child("c").unwrap();
    assert!(core::ptr::eq(child.parent().unwrap(), &PARENT));
    assert!(child.is_enabled(Severity::Debug));
}
