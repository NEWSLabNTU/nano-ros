//! Phase 212.C HARD constraint: `src/` MUST be ≤ 500 LoC (`tokei`).

use std::{path::PathBuf, process::Command};

#[test]
fn tokei_loc_under_500() {
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let output = Command::new("tokei")
        .args(["--output", "json", "--types", "Rust"])
        .arg(&src)
        .output();
    let Ok(out) = output else {
        eprintln!("tokei not on PATH; skipping LoC budget enforcement");
        return;
    };
    assert!(
        out.status.success(),
        "tokei failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let parsed: serde_json::Value = serde_json::from_slice(&out.stdout).expect("tokei JSON output");
    let code = parsed
        .get("Rust")
        .and_then(|r| r.get("code"))
        .and_then(|c| c.as_u64())
        .unwrap_or_else(|| panic!("no Rust.code key in tokei output: {parsed}"));
    assert!(
        code <= 500,
        "src/ exceeds the Phase 212.C 500-LoC cap: {code} LoC"
    );
}
