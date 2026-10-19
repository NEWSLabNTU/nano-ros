//! Issue 1982 -- extra sessions are sized from the backends an image
//! registers. Drives the SAME rule the build script runs
//! (`build/extra_sessions.rs`, included here by `#[path]`).

#[path = "../build/extra_sessions.rs"]
mod extra_sessions;

use extra_sessions::extra_session_cap;

#[test]
fn a_one_rmw_image_carves_no_extra_session() {
    assert_eq!(extra_session_cap(Some(1)), Some(0));
}

#[test]
fn an_image_with_more_backends_carves_one_per_other_backend() {
    assert_eq!(extra_session_cap(Some(2)), Some(1));
    assert_eq!(extra_session_cap(Some(4)), Some(3));
}

#[test]
fn an_unstated_backend_count_keeps_one_per_node() {
    assert_eq!(extra_session_cap(None), None);
}

#[test]
fn a_zero_backend_count_is_not_an_underflow() {
    assert_eq!(extra_session_cap(Some(0)), Some(0));
}
