//! RFC-0114 D13.1 (phase-585 W1) -- the evidence ledger: which manifest row a
//! running test resolved, written as it resolves it.
//!
//! A test's board is the registry row of the coordinate it resolves, never a
//! list a person keeps. The fixture-resolution chokepoint already attributes
//! every artifact to its manifest row for lane skipping
//! ([`super::lane::require_in_lane`], [`super::lane::require_workspace_in_lane`]);
//! when `NROS_TEST_EVIDENCE_DIR` is set, it also appends one line here. CI's
//! collector (`scripts/ci/board-results.py`) joins the lines with the junit.
//!
//! One JSON object per line, in `<dir>/<pid>.jsonl`:
//! `{"test": …, "row": …, "kind": …, "platform": …, "lang": …, "rmw": …}`.
//! The test is the harness's thread name (libtest names a test's thread after
//! it; nextest runs one test per process). Unset: nothing is written.

use std::io::Write as _;

use super::lane::Row;

/// The directory the ledger is written into; unset means no ledger.
pub const EVIDENCE_DIR_ENV: &str = "NROS_TEST_EVIDENCE_DIR";

/// Whether a ledger is wanted (the variable is set and not empty), so a caller
/// can skip attributing a row nobody will record.
pub fn enabled() -> bool {
    std::env::var_os(EVIDENCE_DIR_ENV).is_some_and(|d| !d.is_empty())
}

/// Record that the running test resolved `row`. Never fails the test: a
/// ledger that cannot be written is reported on stderr, and the collector
/// then sees no record for the test, which it reports as missing evidence.
pub fn record(row: &Row) {
    let Some(dir) = std::env::var_os(EVIDENCE_DIR_ENV).filter(|d| !d.is_empty()) else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    if let Err(e) = write_line(&dir, &line(row)) {
        eprintln!(
            "nros-tests: cannot write the evidence ledger in {}: {e}",
            dir.display()
        );
    }
}

/// The ledger line for `row`, for the running test.
pub fn line(row: &Row) -> String {
    let (platform, lang, rmw) = &row.coord;
    serde_json::json!({
        "test": std::thread::current().name().unwrap_or("unknown"),
        "binary": binary(),
        "row": row.id,
        "kind": row.kind,
        "platform": platform,
        "lang": lang,
        "rmw": rmw,
    })
    .to_string()
}

/// The running test binary, as nextest's binary id names it: the executable's
/// name without cargo's `-<hash>` suffix (`owned_entry_run_verb`).
fn binary() -> String {
    let stem = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .unwrap_or_default();
    strip_hash(&stem).to_string()
}

fn strip_hash(stem: &str) -> &str {
    match stem.rsplit_once('-') {
        Some((name, hash)) if hash.len() == 16 && hash.bytes().all(|b| b.is_ascii_hexdigit()) => {
            name
        }
        _ => stem,
    }
}

fn write_line(dir: &std::path::Path, line: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("{}.jsonl", std::process::id()));
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    // One write per line, so lines from concurrent tests in one process do
    // not interleave.
    f.write_all(format!("{line}\n").as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row() -> Row {
        Row {
            kind: "workspace_fixture".into(),
            coord: ("threadx-linux".into(), "c".into(), "zenoh".into()),
            dir: "examples/threadx-linux/c/talker".into(),
            id: "threadx-linux-c-zenoh-talker".into(),
            artifact_root: String::new(),
            serves: Vec::new(),
        }
    }

    #[test]
    fn a_line_names_the_test_the_row_and_its_coordinate() {
        let v: serde_json::Value = serde_json::from_str(&line(&row())).unwrap();
        assert_eq!(v["row"], "threadx-linux-c-zenoh-talker");
        assert_eq!(v["platform"], "threadx-linux");
        assert_eq!(v["lang"], "c");
        assert_eq!(v["rmw"], "zenoh");
        assert!(
            v["test"]
                .as_str()
                .unwrap()
                .contains("a_line_names_the_test"),
            "{v}"
        );
    }

    #[test]
    fn the_binary_is_named_without_its_hash() {
        assert_eq!(
            strip_hash("owned_entry_run_verb-0123456789abcdef"),
            "owned_entry_run_verb"
        );
        assert_eq!(strip_hash("lane-run"), "lane-run");
        assert_eq!(strip_hash("x-0123"), "x-0123");
    }

    #[test]
    fn lines_append_one_per_record() {
        let d = tempfile::tempdir().unwrap();
        write_line(d.path(), "a").unwrap();
        write_line(d.path(), "b").unwrap();
        let f = d.path().join(format!("{}.jsonl", std::process::id()));
        assert_eq!(std::fs::read_to_string(f).unwrap(), "a\nb\n");
    }
}
