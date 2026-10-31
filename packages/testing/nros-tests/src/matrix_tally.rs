//! The tally a consolidated matrix test keeps over its cells — issue 2171.
//!
//! Five tests run a whole matrix inside one `#[test]` (`entry_e2e`,
//! `multihost_e2e`, `realtime_tiers_e2e`, `roundtrip_xprocess_e2e`,
//! `sched_dims_applied_e2e`) and each wrote its own count. Two of them counted
//! a row as RAN before it ran, so a lane skip raised inside the row was "ran"
//! and "skipped" at once: tier 2 printed `realtime_tiers: 22 row(s) ran` for
//! four. And every one of them filed that skip under `skipped`, because their
//! `out of lane` column was filled only when the whole PLATFORM was out of
//! lane. Tier 1 and tier 2 narrow by COORDINATE, so an admitted platform's
//! out-of-lane row reaches the fixture resolver and comes back as a
//! `[SKIPPED:lane]` panic — the case that column exists for. Two others
//! printed nothing at all about the rows that skipped that way.
//!
//! One tally, so the classification has one spelling: a cell that returns
//! EXECUTED and passed; a `[SKIPPED:lane]` panic is OUT OF LANE, whether the
//! caller deselected the platform or the resolver deselected the row; any
//! other skip class is SKIPPED; anything else EXECUTED and FAILED.
//!
//! The summary is the only record of which cells a green covered, so it is
//! printed on stdout, and `.config/nextest.toml` shows these binaries' output
//! on success (issue 2004, gate `check-matrix-summary-visible`).

use crate::skip_marker;

/// What happened to one cell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Passed,
    OutOfLane(String),
    Skipped(String),
    Failed(String),
}

impl Outcome {
    /// Classify a caught panic message.
    pub fn of_panic(msg: String) -> Self {
        match skip_marker::class_in(&msg) {
            Some("lane") => Outcome::OutOfLane(msg),
            Some(_) => Outcome::Skipped(msg),
            None => Outcome::Failed(msg),
        }
    }
}

/// Counts and notes for one matrix test.
#[derive(Debug)]
pub struct Tally {
    name: &'static str,
    unit: &'static str,
    passed: usize,
    out_of_lane: Vec<String>,
    skipped: Vec<String>,
    failed: Vec<String>,
}

impl Tally {
    /// `name` prefixes every line (`realtime_tiers`); `unit` is what a cell is
    /// called in the summary (`row`, `cell`).
    pub fn new(name: &'static str, unit: &'static str) -> Self {
        Self {
            name,
            unit,
            passed: 0,
            out_of_lane: Vec::new(),
            skipped: Vec::new(),
            failed: Vec::new(),
        }
    }

    /// A cell the caller deselected without running it — a
    /// `lane_scope::skip_note`.
    pub fn out_of_lane(&mut self, note: String) {
        self.out_of_lane.push(note);
    }

    /// Run one cell, catching its panic, and record what it did.
    pub fn run(&mut self, label: &str, cell: impl FnOnce()) {
        let prev_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(cell));
        std::panic::set_hook(prev_hook);
        let outcome = match res {
            Ok(()) => Outcome::Passed,
            Err(p) => Outcome::of_panic(
                p.downcast_ref::<String>()
                    .cloned()
                    .or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string()))
                    .unwrap_or_else(|| "<non-string panic>".to_string()),
            ),
        };
        self.record(label, outcome);
    }

    /// Record a cell's outcome under `label`.
    pub fn record(&mut self, label: &str, outcome: Outcome) {
        match outcome {
            Outcome::Passed => self.passed += 1,
            Outcome::OutOfLane(msg) => self.out_of_lane.push(format!("{label}: {msg}")),
            Outcome::Skipped(msg) => self.skipped.push(format!("{label}: {msg}")),
            Outcome::Failed(msg) => self.failed.push(format!("{label}: {msg}")),
        }
    }

    /// Cells that actually ran: passed or failed. Never a deselected one.
    pub fn executed(&self) -> usize {
        self.passed + self.failed.len()
    }

    /// The summary line plus one line per cell that did not pass.
    pub fn summary(&self) -> String {
        let mut s = format!(
            "{}: {} {}(s) executed ({} passed, {} failed), {} out of lane, {} skipped",
            self.name,
            self.executed(),
            self.unit,
            self.passed,
            self.failed.len(),
            self.out_of_lane.len(),
            self.skipped.len(),
        );
        for note in self.out_of_lane.iter().chain(self.skipped.iter()) {
            s.push_str("\n  - ");
            s.push_str(note);
        }
        s
    }

    /// Print the summary, then fail on any failed cell, and lane-skip the test
    /// when no cell executed (issue 1758: every unmet precondition inside a
    /// cell is a FAILURE, so what is left unexecuted is deselection).
    pub fn finish(self) {
        println!("{}", self.summary());
        assert!(
            self.failed.is_empty(),
            "{}: {} of {} {}(s) FAILED:\n  {}",
            self.name,
            self.failed.len(),
            self.executed(),
            self.unit,
            self.failed.join("\n  ")
        );
        if self.executed() == 0 {
            crate::lane_skip!(
                "no {} {} executed ({} out of lane, {} skipped):\n  {}",
                self.name,
                self.unit,
                self.out_of_lane.len(),
                self.skipped.len(),
                self.out_of_lane
                    .iter()
                    .chain(self.skipped.iter())
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("\n  ")
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn panic_msg(f: impl FnOnce() + std::panic::UnwindSafe) -> String {
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let p = std::panic::catch_unwind(f).expect_err("must panic");
        std::panic::set_hook(prev);
        p.downcast_ref::<String>()
            .cloned()
            .or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string()))
            .expect("a string panic payload")
    }

    /// Issue 2171 itself: an admitted platform's row that the resolver
    /// deselects is OUT OF LANE and was never executed.
    #[test]
    fn a_lane_skip_inside_a_cell_is_out_of_lane_not_executed() {
        let mut t = Tally::new("m", "row");
        t.run("a/c", || {});
        t.run("b/cpp", || {
            crate::lane_skip!("out of lane: x is at coordinate b,cpp,zenoh")
        });
        t.out_of_lane("c/rust: out of lane".into());
        assert_eq!(t.executed(), 1);
        assert_eq!(t.out_of_lane.len(), 2);
        assert!(t.skipped.is_empty() && t.failed.is_empty());
        assert!(
            t.summary()
                .starts_with("m: 1 row(s) executed (1 passed, 0 failed), 2 out of lane, 0 skipped"),
            "{}",
            t.summary()
        );
    }

    #[test]
    fn an_unmet_precondition_is_a_failure_that_executed() {
        let mut t = Tally::new("m", "cell");
        t.run("a", || crate::unmet!("no qemu-system-aarch64"));
        assert_eq!(t.executed(), 1);
        assert_eq!(t.failed.len(), 1);
        let msg = panic_msg(move || t.finish());
        assert!(msg.contains("m: 1 of 1 cell(s) FAILED"), "{msg}");
    }

    #[test]
    fn another_skip_class_is_skipped_not_out_of_lane() {
        // Spelled from the prefix: `lane_skip!` is the only producer of a
        // marker (issue 1900), and a second class has no producer at all.
        let other = format!("{}:resource] no port", skip_marker::PREFIX);
        assert_eq!(Outcome::of_panic(other.clone()), Outcome::Skipped(other));
        let lane = panic_msg(|| crate::lane_skip!("out"));
        assert_eq!(Outcome::of_panic(lane.clone()), Outcome::OutOfLane(lane));
        assert_eq!(
            Outcome::of_panic("boom".into()),
            Outcome::Failed("boom".into())
        );
    }

    #[test]
    fn nothing_executed_is_a_lane_skip() {
        let mut t = Tally::new("m", "row");
        t.run("a", || crate::lane_skip!("out of lane: a"));
        let msg = panic_msg(move || t.finish());
        assert!(skip_marker::starts_with_skip(&msg), "{msg}");
        assert!(
            msg.contains("no m row executed (1 out of lane, 0 skipped)"),
            "{msg}"
        );
    }

    #[test]
    fn one_passing_cell_finishes_green() {
        let mut t = Tally::new("m", "row");
        t.run("a", || {});
        t.run("b", || crate::lane_skip!("out of lane: b"));
        t.finish();
    }
}
