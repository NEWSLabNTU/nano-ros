//! phase-560 W8 (#3758, #3759) -- the first mode-form contract: a hazard that
//! enters a mode, and a fallback sequence of several levels.
//!
//! `fixtures/mode_ladder` is a reduced L4 safety island ladder
//! (`lane_keep_hold`, a dwelling level entered only on the loss of the HPC
//! with its own `within`; `pull_over`; `blind_stop`, the minimal risk
//! condition) and three hazards that enter it, one per way rlm v0.3.18 picks
//! the levels a hazard runs. It is resolved the way a build resolves it,
//! through the pinned `nros-launch-resolve`, and read by the two halves the
//! image has:
//!
//! * the build-time budget (`ftti_budget`): one row per level run, each judged
//!   against the deadlines rlm names, and the binding row;
//! * the run-time level report (`hazard-level-runtime`): the rung table the
//!   executor walks, whose names are what it reports.
//!
//! The expected rows are play_launch 0.34.0's on the same files, measured
//! with `play_launch check --explain ladder.launch.xml` (rlm v0.3.18):
//!
//! ```text
//! HAZARD       DEGRADATION     ROLE     DETECT   DWELLS  ROUTE   SETTLE     TOTAL      FTTI  WITHIN
//! camera_loss  lane_keep_hold  skipped
//! camera_loss  pull_over       skipped
//! camera_loss  blind_stop      mrc      150.00     0.00   7.00  3000.00   3157.00  10000.00       -
//! hpc_loss     lane_keep_hold  dwell     30.00     0.00   7.00    50.00     87.00         -  100.00
//! hpc_loss     pull_over       level     30.00  5007.00  21.00 20000.00  25058.00  60000.00       -
//! hpc_loss     blind_stop      mrc       30.00  5007.00  26.00  3000.00   8063.00  60000.00       -
//! radar_loss   lane_keep_hold  skipped
//! radar_loss   pull_over       level    150.00     0.00   7.00 20000.00  20157.00  60000.00       -
//! radar_loss   blind_stop      mrc      150.00     0.00   7.00  3000.00   3157.00  60000.00       -
//!
//! camera_loss  'blind_stop' (mrc)  3157.00 against ftti 10000.00: 32%
//! hpc_loss  'lane_keep_hold' (dwell)  87.00 against within 100.00: 87%
//! radar_loss  'pull_over' (level)  20157.00 against ftti 60000.00: 34%
//! ```
//!
//! `radar_loss` is the case #3758 reported on the L4 contract: the binding
//! row is a level above the minimal risk condition, which a budget charging
//! only the last level never sees.
//!
//! Run with:
//! `cargo test --manifest-path packages/cli/Cargo.toml --test mode_ladder_rows`

mod common;

use std::{
    fs,
    path::{Path, PathBuf},
    process::Output,
};

use nros_orchestration_ir::{
    contract_monitors,
    ftti_budget::{self, Deadline},
};
use ros_launch_manifest_model::{LevelRole, SystemModel};

const SCOPE: &str = "ladder.launch.xml";

/// The model's budgets, keyed by the unscoped hazard name.
fn budget<'b>(bs: &'b [ftti_budget::HazardBudget], hazard: &str) -> &'b ftti_budget::HazardBudget {
    let key = format!("{SCOPE}/{hazard}");
    bs.iter()
        .find(|b| b.hazard == key)
        .unwrap_or_else(|| panic!("no budget for `{key}`: {bs:#?}"))
}

/// `(level, role, total)` of every row.
fn rows(b: &ftti_budget::HazardBudget) -> Vec<(&str, LevelRole, Option<f64>)> {
    b.levels
        .iter()
        .map(|l| (l.level.as_str(), l.role, l.total_ms))
        .collect()
}

/// The build-time rows equal play_launch 0.34.0's, row for row (the table in
/// the module doc), and so does every binding row.
#[test]
fn every_level_row_and_binding_row_match_play_launch() {
    let m = resolve(&fixture_dir(), "ladder");
    let bs = ftti_budget::check_model(&m).expect("every judged pair fits");
    assert_eq!(bs.len(), 3, "{bs:#?}");

    let camera = budget(&bs, "camera_loss");
    assert_eq!(rows(camera), [("blind_stop", LevelRole::Mrc, Some(3157.0))]);

    let hpc = budget(&bs, "hpc_loss");
    assert_eq!(
        rows(hpc),
        [
            ("lane_keep_hold", LevelRole::Dwell, Some(87.0)),
            ("pull_over", LevelRole::Level, Some(25058.0)),
            ("blind_stop", LevelRole::Mrc, Some(8063.0)),
        ]
    );
    // The dwell is waited out at its range's `max` (5 s), route included.
    assert_eq!(hpc.levels[1].waited_ms, Some(5007.0));
    assert_eq!(hpc.levels[0].within_ms, Some(100.0));

    let radar = budget(&bs, "radar_loss");
    assert_eq!(
        rows(radar),
        [
            ("pull_over", LevelRole::Level, Some(20157.0)),
            ("blind_stop", LevelRole::Mrc, Some(3157.0)),
        ]
    );

    let binding = |b: &ftti_budget::HazardBudget| {
        let j = b.binding().expect("a binding pair");
        (
            b.levels[j.row].level.clone(),
            j.deadline,
            j.total_ms,
            j.deadline_ms,
        )
    };
    assert_eq!(
        binding(camera),
        ("blind_stop".into(), Deadline::Ftti, 3157.0, 10000.0)
    );
    assert_eq!(
        binding(hpc),
        ("lane_keep_hold".into(), Deadline::Within, 87.0, 100.0)
    );
    assert_eq!(
        binding(radar),
        ("pull_over".into(), Deadline::Ftti, 20157.0, 60000.0)
    );
}

/// The run-time half: the rung table the executor walks lists the levels
/// each hazard runs, in order, by the names `hazard-level-runtime` reports,
/// and none it skips.
#[test]
fn the_level_report_names_the_levels_each_hazard_runs() {
    let m = resolve(&fixture_dir(), "ladder");
    let rows = contract_monitors::monitor_rows(&m).expect("monitor rows");
    let ages = contract_monitors::age_rows(&m).expect("age rows");
    let hazards = contract_monitors::hazard_rows(&m, &rows, &ages);
    let rungs = |h: &str| -> Vec<(String, u8)> {
        let key = format!("{SCOPE}/{h}");
        hazards
            .iter()
            .find(|r| r.key == key)
            .unwrap_or_else(|| panic!("hazard `{key}` is lowered: {hazards:#?}"))
            .rungs
            .iter()
            .map(|r| (r.name.clone(), r.role))
            .collect()
    };
    assert_eq!(rungs("camera_loss"), [("blind_stop".into(), 2)]);
    assert_eq!(
        rungs("hpc_loss"),
        [
            ("lane_keep_hold".into(), 1),
            ("pull_over".into(), 0),
            ("blind_stop".into(), 2)
        ]
    );
    assert_eq!(
        rungs("radar_loss"),
        [("pull_over".into(), 0), ("blind_stop".into(), 2)]
    );
}

/// The verdict on a missed `within`: the same contract with the island's
/// 70 ms to take authority. play_launch 0.34.0's `check` exits 1 with one
/// error, `level-within` on `hpc_loss`'s `lane_keep_hold` (87 ms), and the
/// pinned resolver refuses the contract on the same row.
#[test]
fn a_missed_within_is_refused_on_the_same_row() {
    let repo = repo_root();
    let dir = temp_output(&repo, "late");
    let launch = dir.join("launch");
    fs::create_dir_all(&launch).expect("create fixture copy");
    let src = fixture_dir().join("launch");
    fs::copy(
        src.join("ladder.launch.xml"),
        launch.join("ladder.launch.xml"),
    )
    .expect("copy launch");
    let contract = fs::read_to_string(src.join("ladder.contract.yaml")).expect("read contract");
    assert!(
        contract.contains("    within: 100ms\n"),
        "the fixture's `within` moved"
    );
    fs::write(
        launch.join("ladder.contract.yaml"),
        contract.replace("    within: 100ms\n", "    within: 70ms\n"),
    )
    .expect("write contract");

    let out = run_resolver(&dir, "ladder", &dir.join("system_model.yaml"));
    let _ = fs::remove_dir_all(&dir);
    assert!(!out.status.success(), "a missed `within` must refuse");
    let stderr = String::from_utf8_lossy(&out.stderr);
    let errors: Vec<&str> = stderr
        .lines()
        .filter(|l| l.starts_with("error: ["))
        .collect();
    assert_eq!(errors.len(), 1, "{stderr}");
    for needle in [
        "[level-within]",
        "hazard 'hpc_loss'",
        "level 'lane_keep_hold' is entered 87.00ms",
        "`within: 70.00ms` by 17.00ms",
    ] {
        assert!(errors[0].contains(needle), "names `{needle}`: {stderr}");
    }
}

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mode_ladder")
}

fn run_resolver(bringup: &Path, stem: &str, model: &Path) -> Output {
    std::process::Command::new(common::pinned_launch_resolver())
        .arg(bringup.join(format!("launch/{stem}.launch.xml")))
        .arg("--bringup-root")
        .arg(bringup)
        .arg("-o")
        .arg(model)
        .output()
        .expect("spawn nros-launch-resolve")
}

/// Resolve `<bringup>/launch/<stem>.launch.xml` through the pinned resolver,
/// by ABSOLUTE path (issue 0285).
fn resolve(bringup: &Path, stem: &str) -> SystemModel {
    let out = temp_output(&repo_root(), stem);
    fs::create_dir_all(&out).expect("create model out dir");
    let model = out.join("system_model.yaml");
    let output = run_resolver(bringup, stem, &model);
    assert!(
        output.status.success(),
        "nros-launch-resolve failed for {stem}:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let text = fs::read_to_string(&model).expect("read the resolved model");
    let _ = fs::remove_dir_all(&out);
    SystemModel::from_yaml_str(&text).expect("the resolved model parses")
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("repo root")
        .to_path_buf()
}

/// Unique scratch dir under the repo's gitignored `tmp/`.
fn temp_output(repo: &Path, name: &str) -> PathBuf {
    let dir = repo.join("tmp").join(format!(
        "mode-ladder-{name}-{}-{}",
        std::process::id(),
        common::unique_stamp()
    ));
    let _ = fs::remove_dir_all(&dir);
    dir
}
