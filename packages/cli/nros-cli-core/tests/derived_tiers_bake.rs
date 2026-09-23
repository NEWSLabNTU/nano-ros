//! phase-459 W1 (issue 1426) - the cmake road reaches the rate-monotonic
//! derivation.
//!
//! The fixture is `examples/workspaces/derived-tiers-cpp` (phase-459 W0): four
//! `SHAPE rclcpp` C++ components in the shape of the Autoware Safety Island,
//! one wall timer each, two at 30 Hz and two at 10 Hz, `CALLBACK_GROUPS main`
//! on every registration, and a `system.toml` that authors no `group_tiers`
//! and no `[tiers.*]`.
//!
//! What issue 1426 measured on exactly that shape: the derivation is complete
//! code with tests, and no authored input reaches it. `codegen-system`
//! collected groups from `[[component]].group_tiers` (which a workspace
//! deriving its schedule does not write) and from `cfg.component_packages`
//! (empty for every workspace with no root `Cargo.toml`), so all four nodes
//! arrived at `derive_tiers_from_contracts` groupless, every one of them went
//! into `groupless_notes`, and the derived schedule was empty. W1 adds the
//! third source - the `nros-metadata.json` a configure writes from the cmake
//! keyword - and this asserts the before and the after on one fixture.
//!
//! # Why the metadata is written here rather than configured
//!
//! Producing it for real means a `cmake configure` of four C++ packages
//! against a provisioned toolchain, and this repo does not compile inside
//! tests. The document below is the one `_nros_metadata_emit()`
//! (`cmake/NanoRosNodeRegister.cmake`) writes, field for field; the island's
//! own `build-board/nros-metadata.json` is byte-compatible with it, and W0's
//! coverage case pins the other half - that each `CMakeLists.txt` really does
//! carry `CALLBACK_GROUPS main`, so the rows this test writes are the rows a
//! configure would.
//!
//! # What is asserted, and what is deliberately not
//!
//! RANKS and MEMBERSHIP, never a kernel priority number. `rank_to_priority`
//! currently maps dense rank 0 to Zephyr priority 0, which outranks the
//! transport threads that feed it; phase-459 W4 moves the whole table into the
//! board's application pool. A test that pinned 0 and 1 here would have to be
//! rewritten by the wave that fixes the defect, and would meanwhile read as if
//! 0 were the intended answer.

mod common;

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use nros_cli_core::orchestration::{
    cargo_metadata_schema::SystemToml, model_ingest, nros_config::NrosConfig,
    tier_resolver::collect_callback_groups,
};
use nros_orchestration_ir::derive::{DerivedSchedule, derive_tiers_from_contracts};
use ros_launch_manifest_model::SystemModel;

/// The two 30 Hz components, in the island's naming.
const FAST: [&str; 2] = ["mrm_emergency_stop_operator", "stop_mode_operator"];
/// The two 10 Hz components.
const SLOW: [&str; 2] = ["mrm_comfortable_stop_operator", "mrm_handler"];

/// The board this bake is for. The fixture's `[image.zephyr]` names
/// `native_sim/native/64`, whose descriptor's platform gives this tier key.
const TARGET_RTOS: &str = "zephyr";

fn repo_root() -> PathBuf {
    // <repo>/packages/cli/nros-cli-core/tests/ -> <repo>
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("repo root")
        .to_path_buf()
}

/// A private copy of the W0 fixture, so a test may write a build tree into it.
///
/// Repo rule: temp trees live under `$project/tmp/`, not the system temp dir.
/// The copy is per-process and per-test, because two tests here disagree about
/// whether the workspace has a `nros-metadata.json` at all.
struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn copy(tag: &str) -> Self {
        let repo = repo_root();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = repo.join("tmp").join(format!(
            "derived-tiers-bake-{tag}-{}-{stamp}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("create the fixture copy");
        copy_tree(&repo.join("examples/workspaces/derived-tiers-cpp"), &root);
        Self { root }
    }

    /// The workspace `nros_system_generate()` would pass: the PARENT of the
    /// bringup package, which for this fixture is `<root>/src`. Baking with
    /// `<root>` instead would be the easier test and the wrong one - the
    /// Zephyr road never passes the workspace root, and the reader has to
    /// reach a build tree one level above what it is handed.
    fn bake_workspace(&self) -> PathBuf {
        self.root.join("src")
    }

    fn bringup(&self) -> PathBuf {
        self.root.join("src/demo_bringup")
    }

    /// Write the document a `cmake -B build-board` configure of this workspace
    /// writes, with `CALLBACK_GROUPS main` on all four registrations.
    fn configure(&self) {
        let rows: Vec<String> = [
            (
                "emergency_stop_pkg",
                "mrm_emergency_stop_operator",
                "MrmEmergencyStopOperator",
            ),
            ("stop_mode_pkg", "stop_mode_operator", "StopModeOperator"),
            (
                "comfortable_stop_pkg",
                "mrm_comfortable_stop_operator",
                "MrmComfortableStopOperator",
            ),
            ("mrm_handler_pkg", "mrm_handler", "MrmHandler"),
        ]
        .iter()
        .map(|(pkg, name, class)| {
            format!(
                "    {{\"name\": \"{name}\", \"pkg\": \"{pkg}\", \"class\": \"{pkg}::{class}\", \
                 \"class_header\": \"{pkg}/{class}.hpp\", \"shape\": \"rclcpp\", \
                 \"sources\": [\"src/{class}.cpp\"], \"deploy\": [], \
                 \"pkg_dir\": \"{}/src/{pkg}\", \"lang\": \"cpp\", \
                 \"callback_groups\": [\"main\"]}}",
                self.root.display()
            )
        })
        .collect();
        let doc = format!(
            "{{\n  \"components\": [\n{}\n  ],\n  \"applications\": [\n  ]\n}}\n",
            rows.join(",\n")
        );
        let dir = self.root.join("build-board");
        fs::create_dir_all(&dir).expect("create the build tree");
        fs::write(dir.join("nros-metadata.json"), doc).expect("write nros-metadata.json");
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("mkdir");
    for entry in fs::read_dir(from).expect("read the fixture") {
        let entry = entry.expect("dir entry");
        let dst = to.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            copy_tree(&entry.path(), &dst);
        } else {
            fs::copy(entry.path(), &dst).expect("copy");
        }
    }
}

/// The fixture's launch file + its contract sidecar, through the pinned
/// resolver. The same helper `example_metadata_coverage` uses, and the same
/// reason: the model is a build artifact, so the test makes one.
fn resolve_model(bringup: &Path) -> SystemModel {
    let resolver = common::pinned_launch_resolver();
    let out = bringup.join("model-out");
    fs::create_dir_all(&out).expect("create the model out dir");
    let model = out.join("system_model.yaml");
    let output = std::process::Command::new(&resolver)
        .arg(bringup.join("launch/system.launch.xml"))
        .arg("--bringup-root")
        .arg(bringup)
        .arg("-o")
        .arg(&model)
        .output()
        .expect("spawn nros-launch-resolve");
    assert!(
        output.status.success(),
        "nros-launch-resolve failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let text = fs::read_to_string(&model).expect("read the resolved model");
    SystemModel::from_yaml_str(&text).expect("the resolved model parses")
}

/// The fixture's own `system.toml`, as the bake loads it.
fn system_toml(bringup: &Path) -> SystemToml {
    let raw = fs::read_to_string(bringup.join("system.toml")).expect("read system.toml");
    toml::from_str(&raw).expect("system.toml parses")
}

/// What `codegen-system` does between the two, with nothing in between: collect
/// the groups from the workspace, then derive.
fn derive(fixture: &Fixture, model: &SystemModel, system: &SystemToml) -> DerivedSchedule {
    let cfg = NrosConfig::from_workspace(&fixture.bake_workspace())
        .expect("the fixture's src/ tree loads as a workspace");
    let groups = collect_callback_groups(&cfg, &system.components);
    derive_tiers_from_contracts(model, TARGET_RTOS, &groups)
}

/// `node name -> the Zephyr priority its derived tier carries`, from the
/// schedule's overrides and tiers. Read as an ORDER, never as a number.
fn priorities(derived: &DerivedSchedule) -> BTreeMap<String, i64> {
    let mut out = BTreeMap::new();
    for ov in &derived.overrides {
        let tier_name = &ov
            .callback_groups
            .first()
            .expect("a derived override binds at least one group")
            .tier;
        let tier = derived
            .tiers
            .get(tier_name)
            .unwrap_or_else(|| panic!("override names tier `{tier_name}`, which is not derived"));
        let spec = tier
            .zephyr
            .as_ref()
            .unwrap_or_else(|| panic!("tier `{tier_name}` has no [tiers.*.zephyr] sub-table"));
        out.insert(ov.name.clone(), spec.priority);
    }
    out
}

/// The wave, on the fixture: the keyword reaches the bake, every node is
/// placed, and the placement is rate-monotonic.
#[test]
fn the_cmake_keyword_makes_the_fixture_derive_a_schedule() {
    let fixture = Fixture::copy("keyword");
    fixture.configure();
    let model = resolve_model(&fixture.bringup());
    let system = system_toml(&fixture.bringup());
    let derived = derive(&fixture, &model, &system);

    assert!(
        derived.groupless_notes.is_empty(),
        "every node declares `CALLBACK_GROUPS main`, so none may be groupless: {:?}",
        derived.groupless_notes
    );
    assert_eq!(
        derived.overrides.len(),
        4,
        "four components, four placements: {:?}",
        derived.overrides
    );

    let prio = priorities(&derived);
    let mut placed: Vec<&str> = prio.keys().map(String::as_str).collect();
    placed.sort_unstable();
    let mut want: Vec<&str> = FAST.iter().chain(SLOW.iter()).copied().collect();
    want.sort_unstable();
    assert_eq!(placed, want, "the four members of the derived schedule");

    // RANKS, not numbers (see the module header). Zephyr is
    // `low_number_is_high`, so "more urgent" is a SMALLER priority.
    let fast: Vec<i64> = FAST.iter().map(|n| prio[*n]).collect();
    let slow: Vec<i64> = SLOW.iter().map(|n| prio[*n]).collect();
    assert_eq!(
        fast[0], fast[1],
        "equal periods share a rank (the pinned ranker gives one `fine_group` \
         one rank); the two 30 Hz nodes must land together: {prio:?}"
    );
    assert_eq!(
        slow[0], slow[1],
        "the two 10 Hz nodes must land together: {prio:?}"
    );
    assert!(
        fast[0] < slow[0],
        "rate-monotonic: 30 Hz outranks 10 Hz. Zephyr counts down, so the fast \
         pair must carry the smaller number: {prio:?}"
    );

    // Exactly two ranks over the four nodes - the island's projection, and the
    // shape every later wave's gate reads.
    let ranks: std::collections::BTreeSet<i64> = prio.values().copied().collect();
    assert_eq!(ranks.len(), 2, "two ranks over four nodes: {prio:?}");

    // Every derived tier is a real `[tiers.*]` row the rest of the pipeline can
    // consume: one per node, each carrying the Zephyr sub-table the target
    // selects. (W3 is the wave that names them by rank instead.)
    assert_eq!(
        derived.tiers.len(),
        4,
        "`derive_tiers_from_contracts` names a tier per NODE: {:?}",
        derived.tiers.keys().collect::<Vec<_>>()
    );
}

/// The bake's own count, through the wrapper `codegen-system` calls - the line
/// an operator sees, and the mutation the rest of the pipeline consumes.
#[test]
fn the_bake_reports_the_derived_tiers_and_binds_every_node() {
    let fixture = Fixture::copy("bake");
    fixture.configure();
    let model = resolve_model(&fixture.bringup());
    let mut system = system_toml(&fixture.bringup());
    assert!(
        system.tiers.is_empty(),
        "the fixture authors no tier; derivation only runs on an empty table"
    );

    let cfg = NrosConfig::from_workspace(&fixture.bake_workspace()).expect("workspace");
    let groups = collect_callback_groups(&cfg, &system.components);
    let (derived, warnings) =
        model_ingest::derive_execution_from_contracts(&mut system, &model, TARGET_RTOS, &groups)
            .expect("the derived bake succeeds");

    assert_eq!(derived, 4, "`derived {derived} scheduling tier(s)`");
    assert!(
        warnings.is_empty(),
        "no path declares a deadline or a budget, so the realizer weakens no \
         guarantee: {warnings:?}"
    );
    // The bake writes the derivation back as ordinary rows, which is what makes
    // `resolve_system_tiers` -> `run_tiers` consume it unchanged.
    assert_eq!(system.tiers.len(), 4);
    assert_eq!(system.node_overrides.len(), 4);
    for ov in &system.node_overrides {
        assert_eq!(
            ov.callback_groups.len(),
            1,
            "`CALLBACK_GROUPS main` declares one group: {ov:?}"
        );
        assert_eq!(ov.callback_groups[0].id, "main");
    }
}

/// The negative control: the same fixture with the keyword removed - which,
/// for a source the cmake road carries only in `nros-metadata.json`, is a
/// workspace that never configured one.
///
/// This is the state issue 1426 measured. It must still be a groupless note
/// per node and an empty schedule, because the fix is a new SOURCE and not a
/// new default: a node that declares no group has nothing for the gating
/// executor to bind, and inventing a group for it would place code on a tier
/// nobody asked for.
#[test]
fn without_the_keyword_every_node_is_groupless_and_nothing_derives() {
    let fixture = Fixture::copy("no-keyword");
    let model = resolve_model(&fixture.bringup());
    let system = system_toml(&fixture.bringup());
    let derived = derive(&fixture, &model, &system);

    assert!(
        derived.tiers.is_empty() && derived.overrides.is_empty(),
        "nothing may be derived: {:?}",
        derived.tiers.keys().collect::<Vec<_>>()
    );
    let mut notes: Vec<&str> = derived
        .groupless_notes
        .iter()
        .map(|n| n.rsplit('/').next().unwrap_or(n))
        .collect();
    notes.sort_unstable();
    let mut want: Vec<&str> = FAST.iter().chain(SLOW.iter()).copied().collect();
    want.sort_unstable();
    assert_eq!(
        notes, want,
        "one groupless note per node (issue 1371's persisted form)"
    );
}
