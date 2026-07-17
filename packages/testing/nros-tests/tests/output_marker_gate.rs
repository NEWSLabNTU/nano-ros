//! RFC-0051 / phase-295 W2.b — output-marker literal gate (audit E7).
//!
//! Every stock-demo output marker lives ONCE in `nros-tests/src/output.rs`;
//! test files consume the constants/helpers (`output::*`,
//! `checker::assert_delivery`). A marker string literal in a test body
//! re-creates the phase-277 drift class (banner slimming broke ~10 tests
//! grepping stale literals while delivery worked — archived issues
//! 0157/0164). This gate scans the test sources themselves.
//!
//! Doc comments and non-marker prose are fine — only STRING LITERALS
//! containing a marker are flagged.

use std::path::PathBuf;

const MARKERS: &[&str] = &[
    "Received:",
    "I heard:",
    "Publishing:",
    "Published:",
    "Result of add_two_ints:",
    "Waiting for service requests",
    "Waiting for action goals",
    "Goal accepted by server",
    "Next number in sequence received:",
];

fn tests_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests")
}

#[test]
fn marker_literals_only_in_output_rs() {
    let mut offenders = Vec::new();
    for entry in std::fs::read_dir(tests_dir()).expect("read tests dir") {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        if path.file_name().and_then(|n| n.to_str()) == Some("output_marker_gate.rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("read test source");
        for (lineno, line) in text.lines().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            // Lines that PRODUCE diagnostics (println!/eprintln!) aren't
            // greps — the drift class only bites patterns MATCHED against
            // node output.
            if code.contains("println!") || code.contains("eprintln!") {
                continue;
            }
            for marker in MARKERS {
                // Flag only STRING LITERALS carrying the marker.
                if code.contains(&format!("\"{marker}")) {
                    offenders.push(format!(
                        "{}:{}: literal `{marker}` — use nros_tests::output::*",
                        path.file_name().unwrap().to_string_lossy(),
                        lineno + 1
                    ));
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "output-marker literals outside output.rs (audit E7):\n{}",
        offenders.join("\n")
    );
}
