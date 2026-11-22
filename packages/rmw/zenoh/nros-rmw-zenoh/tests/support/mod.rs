//! Helpers shared by this crate's integration tests.

use std::sync::{Mutex, MutexGuard, PoisonError};

/// Take this process's turn to hold a zenoh session open.
///
/// The zenoh-pico shim keeps per-process session state, and outside a lane
/// that raises it the build's session cap (`zenoh.max_sessions`) is 1. A bare
/// `cargo test` runs a file's tests as threads of one process (nextest runs
/// each in its own), so two tests holding a session at once could collide.
/// A test holds the guard for as long as its session is open: declare it
/// BEFORE the session, so the session drops first. Issue 2715.
pub fn session_turn() -> MutexGuard<'static, ()> {
    static TURN: Mutex<()> = Mutex::new(());
    // A test that failed while holding the turn poisons it; the next one
    // opens its own session regardless.
    TURN.lock().unwrap_or_else(PoisonError::into_inner)
}
