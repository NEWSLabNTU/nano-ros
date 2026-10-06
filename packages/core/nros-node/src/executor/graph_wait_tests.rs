//! phase-444 — what `Executor::wait_for_publishers` / `wait_for_subscribers`
//! decide, as opposed to what they inherit.
//!
//! The LOOP these two run is `WaitBudget`'s, shared with `Client::
//! wait_for_service` and tested there; re-asserting "a budget that expires
//! returns false" here would test the budget twice and this pair not at all.
//!
//! What IS decided here, and so is what this file pins:
//!
//!  1. `Unsupported` is propagated IMMEDIATELY rather than waited out. This
//!     used to be the one place the pair differed from `wait_for_service`,
//!     which treated a backend that cannot answer as "keep waiting" and
//!     reported a timeout (issue 1087), on the argument that for a service
//!     probe not-yet-answerable is transient. It was not: XRCE answers
//!     `Unsupported` on every probe forever, and every C / C++ XRCE client
//!     read the resulting TIMEOUT as "no server" (issue 1686). The server
//!     waits now take the same decision as this pair — see
//!     `ServerVisibility` and the `server_wait_*` tests at the end of this
//!     file — and the one TRANSIENT "cannot say" (a zenoh graph cache that
//!     dropped tokens) is spelled `WouldBlock`, which keeps waiting.
//!     Waiting out the budget would report "none appeared" for a backend
//!     that cannot see the graph at all, collapsing "cannot tell you" into
//!     "not there". RFC-0036 exists to keep those apart.
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

/// Issue 1686 — the classification every server wait loop shares. Only
/// `Unsupported` ends a wait as "cannot know"; any other error is a probe
/// that failed THIS time.
#[test]
fn server_visibility_classifies_the_three_answers() {
    use nros_rmw::TransportError as E;
    assert_eq!(ServerVisibility::of(Ok(true)), ServerVisibility::Visible);
    assert_eq!(ServerVisibility::of(Ok(false)), ServerVisibility::NotYet);
    assert_eq!(
        ServerVisibility::of(Err(E::Unsupported)),
        ServerVisibility::Unknowable
    );
    for transient in [E::WouldBlock, E::Timeout, E::Disconnected] {
        assert_eq!(
            ServerVisibility::of(Err(transient.clone())),
            ServerVisibility::NotYet,
            "{transient:?} is not \"this backend can never answer\""
        );
    }
}

/// Issue 1686 — a backend that can NEVER say whether a server is up (XRCE)
/// ends the wait at once with `Unsupported`, instead of spending the budget
/// and returning `Ok(false)`, which every C / C++ client read as "no server".
#[test]
fn server_wait_propagates_unsupported_instead_of_waiting_it_out() {
    let mut executor = no_graph_executor();
    let started = std::time::Instant::now();
    let ret = wait_for_server_visible(&mut executor, BUDGET, || {
        Err(nros_rmw::TransportError::Unsupported)
    });
    let elapsed = started.elapsed();
    assert!(
        matches!(
            ret,
            Err(NodeError::Transport(nros_rmw::TransportError::Unsupported))
        ),
        "a backend that cannot know must say Unsupported, not \"no server\": {ret:?}"
    );
    assert!(
        elapsed < PROMPT,
        "Unsupported must return AT ONCE, not after the budget. Took {elapsed:?} of {BUDGET:?}"
    );
}

/// Negative control for the test above: a TRANSIENT "cannot say" (a zenoh
/// graph cache that dropped tokens answers `WouldBlock`) keeps waiting, and a
/// server that appears mid-wait is still seen.
#[test]
fn server_wait_keeps_waiting_through_a_transient_answer() {
    let mut executor = no_graph_executor();
    // Counted in PROBES, not elapsed time: `MockSession`'s `spin_once` does
    // not sleep, so the budget can run out in microseconds. What distinguishes
    // "waited" from "returned at once" is whether the loop asked again.
    let short = Duration::from_millis(300);
    let mut asked = 0;
    assert_eq!(
        wait_for_server_visible(&mut executor, short, || {
            asked += 1;
            Err(nros_rmw::TransportError::WouldBlock)
        }),
        Ok(false),
        "a transient answer that never resolves is a timeout"
    );
    assert!(
        asked > 1,
        "a transient answer must be waited on (re-probed), not returned at once: \
         asked {asked} time(s)"
    );

    let mut probes = 0;
    assert_eq!(
        wait_for_server_visible(&mut executor, BUDGET, || {
            probes += 1;
            match probes {
                1 => Err(nros_rmw::TransportError::WouldBlock),
                2 => Ok(false),
                _ => Ok(true),
            }
        }),
        Ok(true),
        "a server that appears mid-wait is seen"
    );
}
