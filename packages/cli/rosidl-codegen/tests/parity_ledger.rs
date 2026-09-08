// The expected-failure ledger for the parity suites — issue 1176.
//
// `parity_test.rs`'s three `test_parse_all_*` tests used to walk a ROS share
// root, parse and generate every definition in it, COLLECT THE FAILURES, print
// them, and pass:
//
//     if !failures.is_empty() {
//         eprintln!("Failed to process {} out of {} std_msgs …", …);
//         for failure in &failures { eprintln!("  {}", failure); }
//         // Don't panic - just report the failures
//         eprintln!("Note: Some failures expected due to parser limitations …");
//     }
//
// That is a different and worse class than issues 1135 and 1160, which were
// unmet PRECONDITIONS reported as PASS. Here the precondition was met, the work
// ran, the failures were real and counted — and the verdict was still green.
// A set that is neither enumerated nor bounded cannot be told apart from a
// regression: breaking forty more messages passed identically, and FIXING all
// of them passed identically too, so nobody would ever learn the limitation was
// gone. The percentage was computed and discarded.
//
// And the report reached nobody. libtest captures the output of a PASSING test
// and `check-cli-tests` runs `cargo test … --quiet` with no `--nocapture`;
// measured on this ROS-less host before the fix, `parity_test` reported
// `17 passed` in 0.00 s with no failure line anywhere. So the numbers belong in
// an assertion, where a failing test prints them, and not in an `eprintln!` on
// the passing path.
//
// This module is that assertion. It is a ratchet in BOTH directions — an
// unlisted failure is red, and a listed entry that now succeeds is red — which
// is the idiom the repo already uses for `.config/gate-registry-baseline.txt`
// and `.config/interop-cells-without-runner.txt`.
#![allow(dead_code)]

use std::{
    collections::{BTreeMap, HashSet},
    path::{Path, PathBuf},
};

/// One interface definition, walked, with the reason it failed if it did.
///
/// `key` is `<package>/<kind>/<file>` relative to a ROS share root, so it is
/// the same string on every host — which is what makes the ledger portable.
#[derive(Debug, PartialEq, Eq)]
pub struct ParityOutcome {
    pub key: String,
    pub failure: Option<String>,
}

/// Walk one `<root>/<package>/msg` directory: read, parse and generate every
/// `.msg` in it, and record a verdict per file.
///
/// Issue 1176 — this is ONE walk. It was three, copied per package into
/// `parity_test.rs` and each ending in the same swallow-the-failures block,
/// which is how one defect came to exist three times.
pub fn walk_message_dir(package: &str, dir: &Path) -> Vec<ParityOutcome> {
    let mut out = Vec::new();
    for entry in walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "msg"))
    {
        let path = entry.path();
        let rel = path.strip_prefix(dir).unwrap_or(path).display().to_string();
        let key = format!("{package}/msg/{rel}");
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();

        let failure = match std::fs::read_to_string(path) {
            Err(e) => Some(format!("read failed: {e}")),
            Ok(content) => match rosidl_parser::parse_message(&content) {
                Err(e) => Some(format!("parse failed: {e:?}")),
                Ok(msg) => {
                    rosidl_codegen::generate_message_package(package, &name, &msg, &HashSet::new())
                        .err()
                        .map(|e| format!("generate failed: {e:?}"))
                }
            },
        };
        out.push(ParityOutcome { key, failure });
    }
    out.sort_by(|a, b| a.key.cmp(&b.key));
    out
}

/// `tests/parity-expected-failures.txt` — the committed ledger.
pub fn expected_failures_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("parity-expected-failures.txt")
}

/// Load the ledger: key -> reason. See the file itself for the format, for the
/// measurement behind its being empty, and for the one case it does not
/// adjudicate.
pub fn load_expected_failures() -> BTreeMap<String, String> {
    let path = expected_failures_path();
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "cannot read the expected-failure ledger {}: {e}. It is COMMITTED, and a \
             missing one must FAIL — defaulting to \"tolerate nothing\" or \"tolerate \
             everything\" is how the set stopped being reviewable in the first place \
             (issue 1176).",
            path.display()
        )
    });
    parse_expected_failures(&text)
}

/// Ledger syntax, split out so it is testable without a filesystem.
fn parse_expected_failures(text: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for (n, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, reason) = match line.split_once('#') {
            Some((k, r)) => (k.trim(), r.trim()),
            None => (line, ""),
        };
        assert!(
            !reason.is_empty(),
            "{}:{}: `{key}` has no reason. Every entry says WHY it is tolerated, or \
             the ledger is just the unexplained set again, one line long (issue 1176).",
            expected_failures_path().display(),
            n + 1
        );
        assert!(
            map.insert(key.to_string(), reason.to_string()).is_none(),
            "{}:{}: `{key}` is listed twice",
            expected_failures_path().display(),
            n + 1
        );
    }
    map
}

/// Hold a walk to the ledger, EXACTLY, in both directions.
///
/// `Ok(n)` means "n tolerated failures, every one of them listed". `Err` is the
/// report, in the ledger's own format so it can be pasted straight in:
///
///   * a failure with no ledger entry — a regression, or a new limitation
///     somebody now has to write down and justify;
///   * a ledger entry whose file was VISITED and SUCCEEDED — the limitation is
///     gone, so the entry has to go, and the credit is worth taking.
///
/// A ledger entry that was never visited is neither: the installed distro does
/// not carry that file, and this repo does not pin which distro that is. It
/// tolerates nothing it should not — a regression is always an UNLISTED
/// failure, so an unvisited entry cannot mask one.
pub fn parity_verdict(
    outcomes: &[ParityOutcome],
    ledger: &BTreeMap<String, String>,
    scope: &str,
) -> Result<usize, String> {
    let mut unexpected = Vec::new();
    let mut tolerated = Vec::new();
    let mut fixed = Vec::new();

    for o in outcomes {
        match (&o.failure, ledger.get(&o.key)) {
            (Some(reason), None) => unexpected.push(format!("{}  # {reason}", o.key)),
            (Some(_), Some(_)) => tolerated.push(o.key.clone()),
            (None, Some(_)) => fixed.push(o.key.clone()),
            (None, None) => {}
        }
    }

    if unexpected.is_empty() && fixed.is_empty() {
        return Ok(tolerated.len());
    }

    let failed = outcomes.iter().filter(|o| o.failure.is_some()).count();
    let mut msg = format!(
        "{scope}: {} definition(s) walked, {failed} failed, {} tolerated by {}.\n",
        outcomes.len(),
        tolerated.len(),
        expected_failures_path().display(),
    );
    if !unexpected.is_empty() {
        msg.push_str(&format!(
            "\n{} FAILURE(S) NOT IN THE LEDGER. Fix the codegen — or, if this is a \
             limitation someone is accepting, add these lines verbatim:\n",
            unexpected.len()
        ));
        for line in &unexpected {
            msg.push_str(&format!("  {line}\n"));
        }
    }
    if !fixed.is_empty() {
        msg.push_str(&format!(
            "\n{} LEDGER ENTRY/ENTRIES THAT NOW SUCCEED. The limitation is gone; \
             delete these lines:\n",
            fixed.len()
        ));
        for key in &fixed {
            msg.push_str(&format!("  {key}\n"));
        }
    }
    Err(msg)
}

/// The whole verdict for one package directory: walk it, hold it to the ledger,
/// and panic with the report on any disagreement.
pub fn assert_message_dir_parity(package: &str, dir: &Path, scope: &str) {
    let outcomes = walk_message_dir(package, dir);
    assert!(
        !outcomes.is_empty(),
        "{scope}: {} holds no .msg files. The directory RESOLVED, so this is not an \
         absent environment — it is a walk that measured nothing, and answering PASS \
         for it claims coverage nobody has (issue 1176).",
        dir.display()
    );
    if let Err(report) = parity_verdict(&outcomes, &load_expected_failures(), scope) {
        panic!("{report}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outcome(key: &str, failure: Option<&str>) -> ParityOutcome {
        ParityOutcome {
            key: key.to_string(),
            failure: failure.map(str::to_string),
        }
    }

    /// Issue 1176 — the ratchet, BOTH WAYS, on any host, with no ROS install.
    ///
    /// The code this replaces could not be demonstrated at all: it printed the
    /// same thing and passed whether the failure set grew, shrank or vanished.
    /// Each of the four states below is a different verdict here.
    #[test]
    fn ledger_decides_both_directions() {
        let ledger =
            parse_expected_failures("# header\nstd_msgs/msg/Weird.msg  # defaults unsupported\n\n");
        assert_eq!(ledger.len(), 1);

        // 1. a listed failure, still failing -> tolerated, and COUNTED.
        assert_eq!(
            parity_verdict(
                &[
                    outcome("std_msgs/msg/Bool.msg", None),
                    outcome("std_msgs/msg/Weird.msg", Some("parse failed")),
                ],
                &ledger,
                "t",
            ),
            Ok(1)
        );

        // 2. an UNLISTED failure -> red. This is the regression the old code
        //    printed to a captured stream and passed over.
        let regressed = parity_verdict(
            &[outcome("std_msgs/msg/Bool.msg", Some("parse failed: Eof"))],
            &ledger,
            "t",
        )
        .expect_err("an unlisted failure must fail the test");
        assert!(
            regressed.contains("std_msgs/msg/Bool.msg  # parse failed: Eof"),
            "the report must be pasteable into the ledger, got:\n{regressed}"
        );

        // 3. a listed entry that now SUCCEEDS -> also red. Without this the
        //    ledger rots, and a fix nobody hears about is how "parser
        //    limitations (default values, etc.)" outlived the limitation.
        let unrotted = parity_verdict(&[outcome("std_msgs/msg/Weird.msg", None)], &ledger, "t")
            .expect_err("a ledger entry that now parses must fail the test");
        assert!(unrotted.contains("NOW SUCCEED"), "got:\n{unrotted}");
        assert!(unrotted.contains("std_msgs/msg/Weird.msg"));

        // 4. a listed entry this host does not carry -> not a verdict either
        //    way; the installed distro is not pinned by this repo.
        assert_eq!(
            parity_verdict(&[outcome("std_msgs/msg/Bool.msg", None)], &ledger, "t"),
            Ok(0)
        );
    }

    /// An entry with no reason is the unexplained set coming back, one line
    /// long.
    #[test]
    fn ledger_entry_without_a_reason_is_rejected() {
        assert!(
            std::panic::catch_unwind(|| parse_expected_failures("std_msgs/msg/X.msg\n")).is_err(),
            "an entry with no reason must be rejected"
        );
    }

    /// The COMMITTED ledger parses, and every entry is shaped like a key some
    /// walk could actually produce. An entry no walk can match tolerates
    /// nothing while reading as though it tolerates something.
    #[test]
    fn committed_ledger_is_well_formed() {
        for key in load_expected_failures().keys() {
            let parts: Vec<&str> = key.split('/').collect();
            assert!(
                parts.len() >= 3 && ["msg", "srv", "action"].contains(&parts[1]),
                "{}: `{key}` is not `<package>/<msg|srv|action>/<file>`, so no walk \
                 can ever match it",
                expected_failures_path().display()
            );
        }
    }

    /// The walk itself, against a synthesized tree — so "a bad definition is
    /// reported as a failure" is exercised on every host rather than only on
    /// one that happens to carry a message our parser chokes on.
    #[test]
    fn walk_reports_per_file_verdicts() {
        let tmp = std::env::temp_dir().join(format!("nros-1176-{}", std::process::id()));
        let dir = tmp.join("std_msgs").join("msg");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("Good.msg"), "int32 x\nstring name \"hi\"\n").unwrap();
        std::fs::write(dir.join("Bad.msg"), "not_a_type[[ x\n").unwrap();

        let outcomes = walk_message_dir("std_msgs", &dir);
        assert_eq!(outcomes.len(), 2, "both files must get a verdict");
        assert_eq!(outcomes[0].key, "std_msgs/msg/Bad.msg");
        assert!(
            outcomes[0].failure.is_some(),
            "a malformed definition must be recorded as a failure"
        );
        assert_eq!(
            outcomes[1],
            ParityOutcome {
                key: "std_msgs/msg/Good.msg".to_string(),
                failure: None
            }
        );

        std::fs::remove_dir_all(&tmp).ok();
    }
}
