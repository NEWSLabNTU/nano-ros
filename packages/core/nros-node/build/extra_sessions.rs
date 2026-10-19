//! How many extra sessions one executor carves -- issue 1982.
//!
//! `build.rs` includes this file with `#[path]`, and so does
//! `tests/extra_sessions.rs`, so the rule the build runs is the rule the test
//! checks. No `env`, no `OUT_DIR`: the build fact is a parameter.
//!
//! An extra session is one a node opens on an RMW backend other than the
//! primary's (`NodeBuilder::rmw`, `Executor::open_multi`). They were carved one
//! per node slot whatever the image links: 528 B a slot plus a 168 B id on
//! thumbv7em, never pushed on a one-RMW image. A session is per BACKEND
//! (zenoh-pico's is a process singleton, and a node naming the primary's
//! backend gets slot 0), and a backend the image does not register cannot be
//! opened, so an image that registers B backends needs at most B - 1.

/// The cap on extra-session slots per executor, or `None` for no cap below
/// the node slots (the shape before issue 1982).
///
/// `backends` is the sizing descriptor's `[image] backend_count`, the count
/// `nros-rmw-cffi` sizes its backend registry from: the cap is `B - 1`. `None`
/// (not stated) keeps one per node.
///
/// A second session on the SAME backend (another locator or domain, on a
/// backend that allows it) is not counted here; `NROS_EXECUTOR_EXTRA_SESSIONS`
/// states it.
pub(crate) fn extra_session_cap(backends: Option<usize>) -> Option<usize> {
    backends.map(|b| b.saturating_sub(1))
}
