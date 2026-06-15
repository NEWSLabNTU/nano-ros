//! Integration test (phase-251 P4, W4.2): stage prebuilt artifacts into a
//! project-shaped directory and exercise the real discovery + analyze pipeline
//! (find_ninja_log / find_timings_html → normalize). No build is run — the
//! artifacts are the checked-in fixtures, mirroring a prebuilt example dir.

use std::path::Path;

use nros_build_profile::{analyze, model::Backend};

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

/// Lay out `dir/build/.ninja_log` and `dir/target/cargo-timings/cargo-timing.html`.
fn stage_project(dir: &Path, ninja: bool, cargo: bool) {
    if ninja {
        let b = dir.join("build");
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(b.join(".ninja_log"), fixture("sample.ninja_log")).unwrap();
    }
    if cargo {
        let t = dir.join("target").join("cargo-timings");
        std::fs::create_dir_all(&t).unwrap();
        std::fs::write(t.join("cargo-timing.html"), fixture("cargo-timing.html")).unwrap();
    }
}

#[test]
fn analyze_discovers_ninja_log_in_build_subdir() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("proj_ninja");
    let _ = std::fs::remove_dir_all(&dir);
    stage_project(&dir, true, false);

    let p = analyze(&dir).expect("artifacts found");
    // build/ has no west/idf/cmake markers → generic Ninja.
    assert_eq!(p.backend, Backend::Ninja);
    assert!(p.stages.iter().any(|s| s.name == "compile"));
    assert!(p.stages.iter().any(|s| s.name == "link"));
    assert!((p.total_s - 21.9).abs() < 1e-6);
}

#[test]
fn analyze_merges_ninja_and_cargo_into_mixed() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("proj_mixed");
    let _ = std::fs::remove_dir_all(&dir);
    stage_project(&dir, true, true);

    let p = analyze(&dir).expect("artifacts found");
    assert_eq!(p.backend, Backend::Mixed);
    // codegen comes only from the cargo run-custom-build unit.
    assert!(p.stages.iter().any(|s| s.name == "codegen"));
    assert!(p.captured_deep);
}

#[test]
fn analyze_returns_none_on_bare_dir() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("proj_empty");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    assert!(analyze(&dir).is_none());
}
