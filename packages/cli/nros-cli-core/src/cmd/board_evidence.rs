//! RFC-0114 D13.5 (phase-585 W5) -- a board's status word, derived from what
//! RAN: the Rust twin of `board_registry.status_from_evidence`
//! (`scripts/lib/board_registry.py`).
//!
//! The two implementations read ONE fixture of histories and expected words,
//! `scripts/lib/testdata/board-status-cases.json`: the Python self-test runs it
//! on the fast line and this module's test runs it too, so `nros board` and the
//! book cannot come to disagree about a board.

use serde::Deserialize;

/// The default window: three periods of the cadence that delivered the pass.
pub const DEFAULT_OUTCOME_WINDOW: u32 = 3;

/// One run's outcome for one board, newest first in a history.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Run {
    pub date: String,
    #[serde(default)]
    pub workflow: Option<String>,
    #[serde(default)]
    pub event: Option<String>,
    /// `passed`, `failed`, `no-verdict` or `not-run`.
    pub outcome: String,
    /// `report` for a maintainer's `nros board report`; absent for CI.
    #[serde(default)]
    pub source: Option<String>,
    /// For a report: `hardware`, `emulator` or `host`.
    #[serde(default)]
    pub runner: Option<String>,
}

/// D13's N: how many runs a history keeps, and how many marks are drawn
/// (`HISTORY_RUNS` in `scripts/release/record-lane-runs.py`).
pub const HISTORY_RUNS: usize = 6;

/// The mark a run's outcome is drawn as, the same map the job summary uses
/// (`MARK` in `scripts/ci/board-results.py`): a run with no verdict is "–",
/// never "✗".
#[must_use]
pub fn outcome_mark(outcome: &str) -> &'static str {
    match outcome {
        "passed" => "✓",
        "failed" => "✗",
        _ => "–",
    }
}

/// The cadence a run's result evidences, or `None` for a dispatched run (a
/// dispatch proves a run, not a schedule).
pub fn run_cadence(run: &Run) -> Option<&'static str> {
    if run.source.as_deref() == Some("report") {
        return Some("manual");
    }
    match (run.workflow.as_deref(), run.event.as_deref()) {
        (Some("weekly.yml"), Some("schedule")) => Some("weekly"),
        (Some("queue.yml" | "merge-runtime.yml"), Some("merge_group")) => Some("merge"),
        (_, Some("schedule")) => Some("daily"),
        _ => None,
    }
}

fn period_days(cadence: &str) -> i64 {
    if cadence == "weekly" { 7 } else { 1 }
}

/// Days since 1970-01-01 of an ISO `YYYY-MM-DD` date (the inverse of
/// `board_support::iso_from_days`), or `None` when it is not one.
pub fn days_from_iso(date: &str) -> Option<i64> {
    let mut it = date.splitn(3, '-');
    let y: i64 = it.next()?.parse().ok()?;
    let m: i64 = it.next()?.parse().ok()?;
    let d: i64 = it.next()?.parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146_097 + doe - 719_468)
}

/// What the word is derived from besides the history.
#[derive(Debug, Clone, Default)]
pub struct Facts<'a> {
    pub infra: bool,
    pub built: bool,
    /// The descriptor's emulator, when the board runs in one.
    pub emulator: Option<&'a str>,
}

/// D13.5's status word for one board on `today`.
pub fn status_from_evidence(
    facts: &Facts<'_>,
    history: &[Run],
    today: &str,
    window: u32,
    stale_days: u32,
) -> String {
    if facts.infra {
        return "Infrastructure".into();
    }
    let today = days_from_iso(today).unwrap_or(0);
    let age = |r: &Run| today - days_from_iso(&r.date).unwrap_or(i64::MIN / 2);
    let scheduled: Vec<&Run> = history
        .iter()
        .filter(|r| matches!(run_cadence(r), Some("merge" | "daily" | "weekly")))
        .collect();
    let place = if facts.emulator.is_some() {
        "in emulator"
    } else {
        "on host"
    };
    for cad in ["merge", "daily", "weekly"] {
        let fresh = scheduled.iter().any(|r| {
            run_cadence(r) == Some(cad)
                && r.outcome == "passed"
                && age(r) < i64::from(window) * period_days(cad)
        });
        if fresh {
            return format!("Verified {place} ({cad})");
        }
    }
    let verdicts: Vec<&&Run> = scheduled
        .iter()
        .filter(|r| r.outcome == "passed" || r.outcome == "failed")
        .collect();
    if verdicts.first().is_some_and(|r| r.outcome == "failed") {
        let start = verdicts
            .iter()
            .take_while(|r| r.outcome == "failed")
            .last()
            .map(|r| r.date.as_str())
            .unwrap_or_default();
        return format!("Failing since {start}");
    }
    if let Some(p) = history
        .iter()
        .find(|r| r.outcome == "passed" && run_cadence(r) != Some("manual"))
    {
        return format!("Not run since {}", p.date);
    }
    if let Some(r) = history
        .iter()
        .find(|r| r.source.as_deref() == Some("report") && r.outcome == "passed")
    {
        if age(r) > i64::from(stale_days) {
            return format!("Builds (last booted {})", r.date);
        }
        if r.runner.as_deref() == Some("hardware") {
            return format!("Boots on hardware, manual ({})", r.date);
        }
        return format!("Boots, manual ({})", r.date);
    }
    if facts.built {
        "Builds".into()
    } else {
        "Not supported".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Deserialize)]
    struct Cases {
        today: String,
        outcome_window: u32,
        cases: Vec<Case>,
    }

    #[derive(Deserialize)]
    struct Case {
        name: String,
        row: serde_json::Value,
        built: bool,
        history: Vec<Run>,
        want: String,
    }

    /// The SAME cases the Python derivation's self-test runs, read at test
    /// time from the repository, so neither implementation has its own copy.
    #[test]
    fn the_shared_status_cases_agree_with_the_python_derivation() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../scripts/lib/testdata/board-status-cases.json");
        let doc: Cases = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert!(doc.cases.len() >= 10, "the fixture lost its cases");
        let mut bad = Vec::new();
        for c in &doc.cases {
            let facts = Facts {
                infra: c.row.get("infra").and_then(serde_json::Value::as_bool) == Some(true),
                built: c.built,
                emulator: c.row.get("emulator").and_then(serde_json::Value::as_str),
            };
            let got = status_from_evidence(&facts, &c.history, &doc.today, doc.outcome_window, 120);
            if got != c.want {
                bad.push(format!("{}: got {got:?}, want {:?}", c.name, c.want));
            }
        }
        assert!(bad.is_empty(), "{}", bad.join("\n"));
    }

    #[test]
    fn days_round_trip_with_the_civil_calendar() {
        assert_eq!(days_from_iso("1970-01-01"), Some(0));
        assert_eq!(days_from_iso("2026-11-22"), Some(20_779));
        assert_eq!(
            days_from_iso("2000-02-29").map(|d| d + 1),
            days_from_iso("2000-03-01")
        );
        assert_eq!(days_from_iso("2026-13-01"), None);
        assert_eq!(days_from_iso("x"), None);
    }

    /// The marks agree with the job summary's: read `MARK` from the
    /// collector's source, so the CLI and the summary cannot drift apart.
    #[test]
    fn the_marks_are_the_job_summarys() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../scripts/ci/board-results.py");
        let src = std::fs::read_to_string(&path).unwrap();
        let line = src
            .lines()
            .find(|l| l.starts_with("MARK = {"))
            .expect("board-results.py lost its MARK map");
        for outcome in ["passed", "failed", "no-verdict", "not-run"] {
            let want = format!("\"{outcome}\": \"{}\"", outcome_mark(outcome));
            assert!(line.contains(&want), "{outcome}: {want} not in {line}");
        }
    }
}
