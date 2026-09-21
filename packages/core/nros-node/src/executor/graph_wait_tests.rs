//! phase-444 — what `Executor::wait_for_publishers` / `wait_for_subscribers`
//! decide, as opposed to what they inherit.
//!
//! The LOOP these two run is `WaitBudget`'s, shared with `Client::
//! wait_for_service` and tested there; re-asserting "a budget that expires
//! returns false" here would test the budget twice and this pair not at all.
//!
//! What IS decided here, and so is what this file pins:
//!
//!  1. `Unsupported` is propagated IMMEDIATELY rather than waited out. This is
//!     the one place the pair deliberately differs from `wait_for_service`,
//!     which treats a backend that cannot answer as "keep waiting" and reports
//!     a timeout (issue 1087). That is right for a service probe, where
//!     not-yet-answerable is transient; it is wrong for the graph, where a
//!     backend with no graph can NEVER answer — so waiting out the budget
//!     would report "none appeared" for a backend that cannot see publishers
//!     at all, collapsing "cannot tell you" into "not there". RFC-0036 exists
//!     to keep those apart.
//!  2. `count == 0` is satisfied without asking the backend at all, because
//!     "at least zero" is true of every graph and needs no discovery — so it
//!     must hold even where the backend cannot answer.
//!
//! A plain `MockSession` is exactly the fixture for both: it keeps the
//! `Session` trait default on every graph slot, which is `Unsupported`.

use super::*;
use crate::mock::MockSession;
use core::time::Duration;

/// A generous budget. Both assertions below are about NOT consuming it, so it
/// has to be long enough that consuming it is unmistakable in the elapsed
/// time — and short enough that a regression does not hang the suite.
const BUDGET: Duration = Duration::from_secs(5);

/// The elapsed ceiling that distinguishes "returned at once" from "waited out
/// the budget". Deliberately loose: the claim is a factor-of-ten difference,
/// not a benchmark, and a tight bound here would flake under a parallel test
/// run (the QEMU-lane lesson — a loaded host is not a broken one).
const PROMPT: Duration = Duration::from_secs(1);

fn no_graph_executor() -> Executor<'static> {
    let cfg = ExecutorConfig::default();
    Executor::from_session_with(MockSession::new(), &cfg)
}

#[test]
fn wait_for_endpoints_propagates_unsupported_instead_of_waiting_it_out() {
    let mut executor = no_graph_executor();

    for publishers in [true, false] {
        let started = std::time::Instant::now();
        let ret = if publishers {
            executor.wait_for_publishers("/chatter", 1, BUDGET)
        } else {
            executor.wait_for_subscribers("/chatter", 1, BUDGET)
        };
        let elapsed = started.elapsed();
        let verb = if publishers {
            "wait_for_publishers"
        } else {
            "wait_for_subscribers"
        };

        assert!(
            matches!(
                ret,
                Err(NodeError::Transport(nros_rmw::TransportError::Unsupported))
            ),
            "{verb}: a backend that cannot read the graph must say Unsupported, \
             not report that nobody appeared: {ret:?}"
        );
        assert!(
            elapsed < PROMPT,
            "{verb}: Unsupported must return AT ONCE, not after the budget — \
             waiting it out is what turns \"cannot tell you\" into \"not there\". \
             Took {elapsed:?} of a {BUDGET:?} budget"
        );
    }
}

#[test]
fn waiting_for_zero_endpoints_needs_no_graph_at_all() {
    // "at least zero publishers" is true of every graph, so it must not
    // depend on being able to READ the graph — this backend cannot, and the
    // answer is still yes. A forward that asked the backend first would
    // return Unsupported here.
    let mut executor = no_graph_executor();

    let started = std::time::Instant::now();
    assert_eq!(
        executor.wait_for_publishers("/chatter", 0, BUDGET),
        Ok(true),
        "count == 0 is satisfied by every graph, including one nobody can read"
    );
    assert_eq!(
        executor.wait_for_subscribers("/chatter", 0, BUDGET),
        Ok(true),
        "count == 0 is satisfied by every graph, including one nobody can read"
    );
    assert!(
        started.elapsed() < PROMPT,
        "count == 0 must short-circuit, not spin"
    );
}
