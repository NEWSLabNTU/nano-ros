//! phase-427 W10 — how far the rclrs talker tutorial is from compiling here.
//!
//! RFC-0089 "Context and `init`, settled" claims: "A ported rclrs `main` then
//! changes in exactly two places, both `?` on calls that can fail here and
//! cannot there (`create_executor`, `spin`)." This file MEASURES that claim
//! instead of asserting it in prose, and the measurement disagrees with the
//! count — see [`EXPECTED`] below, which is the finding, not a target.
//!
//! Two files, one question each:
//!
//! * `rclrs_talker_port/upstream_rclrs_talker.rs.txt` — the upstream text.
//!   Assembled from rclrs's own documentation on `ros2-rust/ros2_rust@main`,
//!   line for line: the `Context::default_from_env` / `create_basic_executor`
//!   / `create_node` / `spin(SpinOptions::default()).first_error()?` skeleton
//!   is the crate-level "Basic Usage" example in `rclrs/src/lib.rs`, and the
//!   `node.create_publisher::<T>("topic")?` line is from
//!   `NodeState::create_publisher`'s own doc example in `rclrs/src/node.rs`
//!   (the crate-level one creates a subscription; a talker publishes). Not
//!   compiled — rclrs is not in this graph.
//!
//! * `rclrs_talker_port/ported_talker.rs` — the same program against nano-ros.
//!   COMPILED, by `include!` below, so "it ports" is a build-time fact and not
//!   a comment. It is the same file the diff reads, so the two cannot drift:
//!   a change to the port that nobody reflects here fails the test.
//!
//! Neither directory file is a cargo test target — cargo discovers `tests/*.rs`
//! and `tests/*/main.rs`, and `tests/rclrs_talker_port/` has neither.

#![cfg(feature = "rclrs-port-test")]

// The port, compiled. `main` here is an ordinary function inside a module, not
// the harness's entry point; `unused_variables` is allowed because upstream's
// example does not use its `publisher` either (it creates the primitive and
// spins), and changing that would be an edit this file exists to count.
#[allow(unused_variables, dead_code)]
mod ported {
    include!("rclrs_talker_port/ported_talker.rs");

    /// The included `main` under a name the harness can reach — inside the
    /// module it is in scope, private or not. Its declared type is the port's
    /// signature, so a change to the ported `main` that this file has not
    /// accounted for fails to compile rather than passing quietly.
    pub const MAIN: fn() -> Result<(), Box<dyn core::error::Error>> = main;
}

const UPSTREAM: &str = include_str!("rclrs_talker_port/upstream_rclrs_talker.rs.txt");
const PORTED: &str = include_str!("rclrs_talker_port/ported_talker.rs");

/// What each differing line costs, and who owns it.
///
/// `(line index, upstream, ported, why)`. Exhaustive: a difference not listed
/// here fails the test, and a listed difference that is no longer there fails
/// it too. That is the point — this is the acceptance measurement for W10, so
/// it must break when either side moves.
const EXPECTED: &[(usize, &str, &str, Kind)] = &[
    (
        0,
        "use rclrs::*;",
        "use nros::*;",
        // Every port has this line. Not an API difference: the glob is the
        // same glob, over a different crate.
        Kind::Import,
    ),
    (
        1,
        "use example_interfaces::msg::String as StringMsg;",
        "use nros_std_msgs_diag::msg::String as StringMsg;",
        // Likewise. Message crates are generated per workspace (RFC-0023), so
        // the path is the user's, not ours.
        Kind::Import,
    ),
    (
        3,
        "fn main() -> Result<(), RclrsError> {",
        "fn main() -> Result<(), Box<dyn core::error::Error>> {",
        // RFC-0089: "the error types differ (`InitError`/`NodeError` for
        // `RclrsError`), invisible under `?` and loud under a `match`".
        // Invisible under `?` is what phase-427 W10 MADE true: `NodeError` had
        // no `Display` and therefore no `core::error::Error`, so before this
        // wave a ported `main` could not have one error type at all — it had
        // to `map_err` at every call. One line, and it is the signature.
        Kind::ErrorType,
    ),
    (
        5,
        "    let mut executor = context.create_basic_executor();",
        "    let mut executor = context.create_executor()?;",
        // THE FIRST EDIT THE RFC PREDICTS. `Context::create_executor` is
        // phase-427 W10; before it, this line had nowhere to go at all, since
        // the executor was opened from an `ExecutorConfig` the caller built by
        // hand. Ours returns `Result` because opening the session is where the
        // transport can fail; upstream's runtime argument
        // (`create_executor(runtime)`, of which `create_basic_executor()` is
        // the default) has no counterpart — RFC-0002 puts one executor per
        // RTOS task and there is no runtime to choose between.
        Kind::PredictedEdit,
    ),
    (
        6,
        "    let node = executor.create_node(\"talker\")?;",
        "    let mut node = executor.create_node(\"talker\")?;",
        // `mut`, and nothing else. Ledger row `rust:Executor::create_node`
        // already records why: ours hands back a borrowing `NodeHandle` from a
        // compile-time-sized table rather than an `Arc<Node>`, so creating an
        // entity on it is `&mut`. Loud: rustc names the binding.
        Kind::Mutability,
    ),
    (
        10,
        "    executor.spin(SpinOptions::default()).first_error()?;",
        "    executor.spin_blocking(SpinOptions::default())?;",
        // THE SECOND EDIT THE RFC PREDICTS — plus a rename it does not.
        // `?` replaces `.first_error()?` because ours returns `Result<(),
        // NodeError>` where upstream returns `Vec<RclrsError>` (no allocator;
        // ledger row `rust:Executor::spin`). The rename is the part W10 does
        // not own: `Executor::spin` is TAKEN here by `spin(Duration) -> !`,
        // the body of an RTOS task, and RFC-0089's target shape moves the
        // `SpinOptions` form onto that name in a later wave. Until then the
        // edit is on the same line, so it costs no extra one.
        Kind::PredictedEdit,
    ),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// The crate a name is imported from. Unavoidable in any port.
    Import,
    /// The `main` signature's error type.
    ErrorType,
    /// A `mut` the borrow checker requires.
    Mutability,
    /// One of the two `?`s RFC-0089 predicts.
    PredictedEdit,
}

fn lines(src: &str) -> Vec<&str> {
    src.lines().collect()
}

#[test]
fn the_port_differs_from_upstream_only_where_this_file_says() {
    let upstream = lines(UPSTREAM);
    let ported = lines(PORTED);
    assert_eq!(
        upstream.len(),
        ported.len(),
        "the two programs must line up line for line, or 'how many lines changed' \
         has no meaning; upstream {} lines, ported {}",
        upstream.len(),
        ported.len()
    );

    let actual: Vec<usize> = (0..upstream.len())
        .filter(|&i| upstream[i] != ported[i])
        .collect();
    let expected: Vec<usize> = EXPECTED.iter().map(|(i, ..)| *i).collect();
    assert_eq!(
        actual, expected,
        "the set of changed lines moved. Upstream:\n{UPSTREAM}\nPorted:\n{PORTED}"
    );

    for &(i, up, port, _) in EXPECTED {
        assert_eq!(
            upstream[i], up,
            "upstream line {i} is not what EXPECTED says"
        );
        assert_eq!(ported[i], port, "ported line {i} is not what EXPECTED says");
    }
}

#[test]
fn exactly_two_of_the_changed_lines_are_the_ones_rfc_0089_predicts() {
    // The RFC's claim, held to its own words: the two `?`s, on
    // `create_executor` and on `spin`. Both are present and both are the only
    // members of their kind — that half of the acceptance holds.
    let predicted: Vec<usize> = EXPECTED
        .iter()
        .filter(|(.., kind)| *kind == Kind::PredictedEdit)
        .map(|(i, ..)| *i)
        .collect();
    assert_eq!(predicted.len(), 2, "RFC-0089 predicts exactly two");
    for i in predicted {
        let (_, _, ported, _) = EXPECTED.iter().find(|(j, ..)| *j == i).unwrap();
        assert!(
            ported.contains(")?;"),
            "line {i} is a predicted edit, so the ported side must end in a `?`: {ported}"
        );
    }
}

#[test]
fn the_total_is_six_and_the_remainder_is_accounted_for() {
    // The honest number, recorded so a future wave that shrinks it has to come
    // back here and say so. "Exactly two edits" is NOT what the port costs
    // today; it is what W10's half of the port costs.
    assert_eq!(EXPECTED.len(), 6, "six lines differ, not two");

    let count = |k: Kind| EXPECTED.iter().filter(|(.., kind)| *kind == k).count();
    // Two imports (any port has them), one error type (which W10's
    // `Display for NodeError` is what makes possible at all), one `mut` the
    // ledger already records as structural, two predicted `?`s.
    assert_eq!(count(Kind::Import), 2);
    assert_eq!(count(Kind::ErrorType), 1);
    assert_eq!(count(Kind::Mutability), 1);
    assert_eq!(count(Kind::PredictedEdit), 2);
}

#[test]
fn the_diffed_text_is_the_program_that_compiles() {
    // ONE path, read two ways: `include!` compiled it, `include_str!` diffed
    // it. So "it ports" is a build-time fact about the same bytes the count
    // above is computed from, not a comment beside a copy.
    //
    // Not CALLED: upstream's `spin(SpinOptions::default())` never returns and
    // neither does ours. What is under test is that it type-checks — which the
    // binding does, since `MAIN`'s declared type is the port's signature.
    let _compiled: fn() -> Result<(), Box<dyn core::error::Error>> = ported::MAIN;
    assert!(
        PORTED.contains("let mut executor = context.create_executor()?;"),
        "the compiled port must be the one that goes through Context::create_executor"
    );
    assert!(
        PORTED.contains("let mut node = executor.create_node(\"talker\")?;"),
        "…and the one that names its node at Executor::create_node"
    );
}
