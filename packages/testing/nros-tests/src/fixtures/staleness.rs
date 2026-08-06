//! What a freshness probe compared, and how long a coordinate has not run.
//!
//! # The defect this addresses (issue 0445)
//!
//! A staleness verdict is ABSORBING. When a probe calls a fixture stale the
//! fixture is never launched, so the runtime result it would have produced is
//! not merely unknown — it is replaced by an explanation that reads as
//! complete, remedy and all. Issue 0444 (a FreeRTOS Rust image that boots,
//! brings up the network and never reaches app setup) sat behind issue 0442 (a
//! probe exemption applied on one arm and not its sibling) for exactly as long
//! as the cells read STALE, and nothing in the report suggested looking.
//!
//! Two properties fix that without weakening any probe:
//!
//! 1. **Say what was compared.** A probe cannot know it is wrong, but it can
//!    account for its own decisions. `zpico.h` being EXAMINED by one arm and
//!    EXEMPTED by another is visible the moment the verdict prints the counts.
//! 2. **Count how long a coordinate has not run.** "Stale since your last edit"
//!    and "stale for the eleventh run in a row" are different signals and used
//!    to print identically. The second is the state in which defects
//!    accumulate unseen, so it is worth a line of its own.
//!
//! # One spelling of the exemption rule
//!
//! [`exempt_probe_input`] is the single place that decides whether a candidate
//! dependency is an edit event. It exists because the three probe arms
//! (`dep_file_newer_than`, `cmake_dep_info_newer_source`,
//! `newest_source_after`) each carried their OWN subset of the exemptions:
//! cargo-`OUT_DIR` products were skipped by one arm only, and the in-place
//! header list by two of three. That divergence IS issue 0442, and issue 0196
//! says the fix is a shared helper rather than a third copy. `check-staleness-
//! probe-exemptions` keeps them on it.

use std::{
    cell::Cell,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{TestError, project_root};

/// Headers that are REGENERATED IN PLACE: cbindgen writes them on every build
/// of any feature set, moving the mtime without changing a byte. "Newer than my
/// binary" therefore says nothing about this fixture's inputs — issue #222's
/// cross-family false-stale.
const REGENERATED_INPLACE_HEADERS: &[&str] = &[
    "packages/api/nros-c/include/nros/nros_generated.h",
    "packages/api/nros-cpp/include/nros/nros_cpp_ffi.h",
    "packages/rmw/zenoh/zpico-sys/c/include/zpico.h",
];

/// Why a candidate dependency is not an edit event.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Exemption {
    /// cbindgen output written in place — mtime moves, content does not.
    RegeneratedInPlace,
    /// A cargo `OUT_DIR` product (`…/build/<pkg>-<hash>/out/…`): ANY cargo
    /// invocation in the same target dir reruns build scripts and refreshes
    /// these WITHOUT relinking the binary. A semantic change to one implies an
    /// edited tracked source, and those ARE in the dep graph. Without this,
    /// `just ci` re-staled the very fixtures its own check phase probed fresh.
    CargoOutDir,
}

impl Exemption {
    fn label(self) -> &'static str {
        match self {
            Exemption::RegeneratedInPlace => "regenerated-in-place header",
            Exemption::CargoOutDir => "cargo OUT_DIR product",
        }
    }
}

/// The one exemption rule. Returns `Some(reason)` when `path` must not be read
/// as an edit event, `None` when it is a real input worth comparing.
///
/// Every probe arm calls THIS — never a subset of it. See the module docs.
pub fn exempt_probe_input(path: &Path) -> Option<Exemption> {
    if REGENERATED_INPLACE_HEADERS
        .iter()
        .any(|suffix| path.ends_with(suffix))
    {
        return Some(Exemption::RegeneratedInPlace);
    }
    if is_cargo_out_dir_product(path) {
        return Some(Exemption::CargoOutDir);
    }
    None
}

fn is_cargo_out_dir_product(path: &Path) -> bool {
    let mut saw_out = false;
    for comp in path.components().rev() {
        let c = comp.as_os_str().to_string_lossy();
        if !saw_out {
            if c == "out" {
                saw_out = true;
            }
        } else {
            // the component just above `out/` is `<pkg>-<hash>` inside `build/`
            return path
                .components()
                .any(|p| p.as_os_str().to_string_lossy() == "build");
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Direction 1 — account for what the probe compared.
// ---------------------------------------------------------------------------

thread_local! {
    static EXAMINED: Cell<usize> = const { Cell::new(0) };
    static EXEMPT_INPLACE: Cell<usize> = const { Cell::new(0) };
    static EXEMPT_OUTDIR: Cell<usize> = const { Cell::new(0) };
}

/// Start accounting for one probe run. Called by each `require_*_fresh` entry
/// point before it probes; the counters are per-thread because nextest runs
/// each test in its own process and rstest fixtures resolve on the test thread.
pub fn begin_probe() {
    EXAMINED.with(|c| c.set(0));
    EXEMPT_INPLACE.with(|c| c.set(0));
    EXEMPT_OUTDIR.with(|c| c.set(0));
}

/// Record one candidate the probe looked at, and whether the shared rule
/// exempted it. Returns `true` when the caller should SKIP the candidate — so
/// an arm reads `if note_candidate(&p) { continue; }` and cannot both account
/// and decide differently.
pub fn note_candidate(path: &Path) -> bool {
    EXAMINED.with(|c| c.set(c.get() + 1));
    match exempt_probe_input(path) {
        Some(Exemption::RegeneratedInPlace) => {
            EXEMPT_INPLACE.with(|c| c.set(c.get() + 1));
            true
        }
        Some(Exemption::CargoOutDir) => {
            EXEMPT_OUTDIR.with(|c| c.set(c.get() + 1));
            true
        }
        None => false,
    }
}

/// What the probe compared, in one line.
pub fn probe_accounting() -> String {
    let examined = EXAMINED.with(Cell::get);
    let inplace = EXEMPT_INPLACE.with(Cell::get);
    let outdir = EXEMPT_OUTDIR.with(Cell::get);
    format!(
        "examined {examined} input(s); exempted {inplace} {} + {outdir} {}",
        Exemption::RegeneratedInPlace.label(),
        Exemption::CargoOutDir.label(),
    )
}

// ---------------------------------------------------------------------------
// Direction 3 — count how long a coordinate has not produced a runtime result.
// ---------------------------------------------------------------------------

/// How long this coordinate has been reported stale.
#[derive(Clone, Copy, Debug)]
pub struct StaleHistory {
    /// Consecutive resolutions that ended in a staleness verdict, this one
    /// included. `1` means "stale since your last edit".
    pub consecutive: u32,
    /// Unix seconds of the FIRST verdict in the current run of them.
    pub since_epoch: u64,
}

impl StaleHistory {
    /// The line worth printing above a remedy: silent when a cell just went
    /// stale, loud once it has been non-running long enough to hide something.
    pub fn note(&self) -> String {
        if self.consecutive < 2 {
            return String::new();
        }
        let age = now_epoch().saturating_sub(self.since_epoch);
        format!(
            "\n  NOT RUN: {}th consecutive stale verdict for this fixture, first {} ago.\n  \
             This coordinate has produced no runtime result since then — whatever it \n  \
             would have done is being absorbed by this message (issue 0445). If the \n  \
             rebuild does not clear it, suspect the probe before trusting the verdict.",
            self.consecutive,
            human_duration(age),
        )
    }
}

fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn human_duration(secs: u64) -> String {
    match secs {
        s if s < 90 => format!("{s}s"),
        s if s < 5400 => format!("{}m", s / 60),
        s if s < 172_800 => format!("{}h", s / 3600),
        s => format!("{}d", s / 86_400),
    }
}

/// Where the ledger lives. Under `target/` (gitignored) so it survives a test
/// run but never a clean, and never reaches a commit.
fn ledger_dir() -> PathBuf {
    project_root().join("target/nros-fixture-staleness")
}

/// One file per coordinate, named after the binary path with separators
/// flattened. Per-coordinate files rather than one shared ledger because
/// nextest resolves fixtures from many processes at once and a single file
/// would need a lock for no benefit.
fn ledger_path(binary: &Path) -> PathBuf {
    let root = project_root();
    let rel = binary.strip_prefix(&root).unwrap_or(binary);
    let key: String = rel
        .to_string_lossy()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect();
    // Long paths (workspace fixtures nest deeply) would blow past NAME_MAX.
    let key = if key.len() > 180 {
        format!("{}__{:x}", &key[key.len() - 140..], fnv1a(key.as_bytes()))
    } else {
        key
    };
    ledger_dir().join(format!("{key}.stale"))
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Record a staleness verdict for `binary` and return the run it belongs to.
///
/// Best-effort: a ledger that cannot be written degrades to "first verdict",
/// which is exactly the pre-0445 behaviour. A probe must never fail because
/// bookkeeping did.
pub fn record_stale(binary: &Path) -> StaleHistory {
    let path = ledger_path(binary);
    let prev = fs::read_to_string(&path).ok().and_then(|s| parse_entry(&s));
    let entry = match prev {
        Some((n, since)) => StaleHistory {
            consecutive: n.saturating_add(1),
            since_epoch: since,
        },
        None => StaleHistory {
            consecutive: 1,
            since_epoch: now_epoch(),
        },
    };
    let _ = fs::create_dir_all(ledger_dir());
    let _ = fs::write(
        &path,
        format!(
            "{} {} {}\n",
            entry.consecutive,
            entry.since_epoch,
            binary.display()
        ),
    );
    entry
}

/// Clear the coordinate's stale run — it resolved fresh, so it is about to
/// produce a runtime result.
pub fn record_fresh(binary: &Path) {
    let _ = fs::remove_file(ledger_path(binary));
}

fn parse_entry(s: &str) -> Option<(u32, u64)> {
    let mut it = s.split_whitespace();
    let n = it.next()?.parse().ok()?;
    let since = it.next()?.parse().ok()?;
    Some((n, since))
}

/// A coordinate currently carrying a stale run, for reporting.
pub struct NonRunning {
    pub binary: String,
    pub consecutive: u32,
    pub since_epoch: u64,
}

/// Every coordinate the ledger currently reports as non-running, most-stuck
/// first. Backs `just fixture-staleness`.
pub fn non_running() -> Vec<NonRunning> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(ledger_dir()) else {
        return out;
    };
    for entry in entries.flatten() {
        let Ok(text) = fs::read_to_string(entry.path()) else {
            continue;
        };
        let mut it = text.split_whitespace();
        let (Some(n), Some(since)) = (it.next(), it.next()) else {
            continue;
        };
        let (Ok(consecutive), Ok(since_epoch)) = (n.parse(), since.parse()) else {
            continue;
        };
        out.push(NonRunning {
            binary: it.collect::<Vec<_>>().join(" "),
            consecutive,
            since_epoch,
        });
    }
    out.sort_by(|a, b| {
        b.consecutive
            .cmp(&a.consecutive)
            .then(a.since_epoch.cmp(&b.since_epoch))
    });
    out
}

// ---------------------------------------------------------------------------
// The verdict itself.
// ---------------------------------------------------------------------------

/// Build the one staleness error every probe returns.
///
/// One spelling so the accounting and the not-run counter cannot be present on
/// some arms and missing on others — the same reason the exemption rule is
/// shared. `what` names the artifact ("Test fixture", "Zephyr fixture"),
/// `newer` is the input that tripped it, `remedy` the rebuild command.
pub fn stale_error(what: &str, binary: &Path, newer: &Path, remedy: &str) -> TestError {
    let history = record_stale(binary);
    TestError::BuildFailed(format!(
        "{what} is STALE — a source is newer than the built binary:\n  \
         binary: {}\n  newer:  {}\n  probe:  {}{}\n\
         {remedy}",
        binary.display(),
        newer.display(),
        probe_accounting(),
        history.note(),
    ))
}

/// A staleness verdict that is not an mtime comparison (the west-fixture CLI
/// stamp). Same ledger, same not-run counter, no accounting line — nothing was
/// compared.
pub fn stale_error_custom(binary: &Path, body: String) -> TestError {
    let history = record_stale(binary);
    TestError::BuildFailed(format!("{body}{}", history.note()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exemptions_are_one_rule_not_three() {
        let inplace = project_root().join("packages/rmw/zenoh/zpico-sys/c/include/zpico.h");
        assert_eq!(
            exempt_probe_input(&inplace),
            Some(Exemption::RegeneratedInPlace),
            "the header whose one-armed exemption was issue 0442"
        );
        let outdir = project_root().join("target/debug/build/zpico-sys-abc123/out/zpico.o");
        assert_eq!(exempt_probe_input(&outdir), Some(Exemption::CargoOutDir));
        let real = project_root().join("packages/core/nros-core/src/lib.rs");
        assert_eq!(exempt_probe_input(&real), None);
    }

    #[test]
    fn accounting_reports_what_was_examined_and_exempted() {
        begin_probe();
        assert!(note_candidate(
            &project_root().join("packages/rmw/zenoh/zpico-sys/c/include/zpico.h")
        ));
        assert!(!note_candidate(
            &project_root().join("packages/core/nros-core/src/lib.rs")
        ));
        let line = probe_accounting();
        assert!(line.contains("examined 2"), "{line}");
        assert!(
            line.contains("exempted 1 regenerated-in-place header"),
            "{line}"
        );
    }

    #[test]
    fn a_fresh_resolution_clears_the_not_run_run() {
        let key = project_root().join("target/nros-fixture-staleness-selftest/bin-a");
        record_fresh(&key);
        assert_eq!(record_stale(&key).consecutive, 1);
        assert_eq!(record_stale(&key).consecutive, 2);
        let second = record_stale(&key);
        assert_eq!(second.consecutive, 3);
        assert!(
            second.note().contains("NOT RUN"),
            "a coordinate stale three runs running must say so: {}",
            second.note()
        );
        record_fresh(&key);
        assert_eq!(
            record_stale(&key).consecutive,
            1,
            "running the fixture must reset the count, else the counter measures \
             age rather than non-running"
        );
        record_fresh(&key);
    }

    #[test]
    fn a_single_stale_verdict_stays_quiet() {
        let key = project_root().join("target/nros-fixture-staleness-selftest/bin-b");
        record_fresh(&key);
        let first = record_stale(&key);
        assert_eq!(first.consecutive, 1);
        assert!(
            first.note().is_empty(),
            "stale-since-your-last-edit is the normal case and must not shout"
        );
        record_fresh(&key);
    }
}
