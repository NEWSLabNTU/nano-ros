//! The `examples/workspaces/derived-tiers-cpp` fixture (phase-459 W0), as the
//! tests of issue 1426 drive it.
//!
//! One spelling. Two test binaries had a private copy of every helper below
//! (`derived_tiers_bake`, `derived_tiers_entry`), they agreed, and issue 1426's
//! own gate would have been the third — which is the shape CLAUDE.md names:
//! a copied idiom is fine right up to the moment it stops agreeing, and the
//! cheapest time to collapse it is when a third copy is about to be written.
//!
//! Nothing here compiles anything. Producing the cmake metadata for real means
//! a configure of four C++ packages against a provisioned toolchain, and this
//! repo does not compile inside tests; [`Fixture::configure`] writes the
//! document `_nros_metadata_emit()` (`cmake/NanoRosNodeRegister.cmake`) writes,
//! field for field, and `example_metadata_coverage` pins the other half — that
//! each `CMakeLists.txt` really does carry `CALLBACK_GROUPS main`.

#![allow(dead_code)]

use std::{
    fs,
    path::{Path, PathBuf},
};

/// The two 30 Hz components, in the island's naming.
pub const FAST: [&str; 2] = ["mrm_emergency_stop_operator", "stop_mode_operator"];
/// The two 10 Hz components.
pub const SLOW: [&str; 2] = ["mrm_comfortable_stop_operator", "mrm_handler"];

/// `(package, node name, class)` for the fixture's four components.
const COMPONENTS: [(&str, &str, &str); 4] = [
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
];

fn repo_root() -> PathBuf {
    // <repo>/packages/cli/nros-cli-core/tests/ -> <repo>
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("repo root")
        .to_path_buf()
}

/// A private copy of the W0 fixture, so a test may write a build tree into it
/// and edit its `system.toml`.
///
/// Repo rule: temp trees live under `$project/tmp/`, not the system temp dir.
/// The copy is per-process and per-test, because these tests disagree about
/// whether the workspace has a `nros-metadata.json` at all and about what its
/// `system.toml` says.
pub struct Fixture {
    pub root: PathBuf,
}

impl Fixture {
    pub fn copy(tag: &str) -> Self {
        let repo = repo_root();
        let stamp = super::unique_stamp();
        let root = repo.join("tmp").join(format!(
            "derived-tiers-{tag}-{}-{stamp}",
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
    pub fn bake_workspace(&self) -> PathBuf {
        self.root.join("src")
    }

    pub fn bringup(&self) -> PathBuf {
        self.root.join("src/demo_bringup")
    }

    /// Write the document a `cmake -B build-board` configure of this workspace
    /// writes. `groups` is what `CALLBACK_GROUPS` put in it; an empty slice is
    /// the keyword absent, which for a source the cmake road carries only here
    /// is the state issue 1426 measured. Returns the file's path (what
    /// `nano_ros_add_executable` hands the entry bake as `--metadata`).
    pub fn configure(&self, groups: &[&str]) -> PathBuf {
        let ids = groups
            .iter()
            .map(|g| format!("\"{g}\""))
            .collect::<Vec<_>>()
            .join(", ");
        let rows: Vec<String> = COMPONENTS
            .iter()
            .map(|(pkg, name, class)| {
                format!(
                    "    {{\"name\": \"{name}\", \"pkg\": \"{pkg}\", \"class\": \"{pkg}::{class}\", \
                     \"class_header\": \"{pkg}/{class}.hpp\", \"shape\": \"rclcpp\", \
                     \"sources\": [\"src/{class}.cpp\"], \"deploy\": [], \
                     \"pkg_dir\": \"{}/src/{pkg}\", \"lang\": \"cpp\", \
                     \"callback_groups\": [{ids}]}}",
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
        let path = dir.join("nros-metadata.json");
        fs::write(&path, doc).expect("write nros-metadata.json");
        path
    }

    /// Append to the fixture's `system.toml`, and optionally bind one group of
    /// each named component to `tier`.
    ///
    /// `bind` names the components that get `group_tiers = { main = "<tier>" }`
    /// on their `[[component]]` row; `append` is written verbatim at the end
    /// (the `[tiers.*]` the test is about).
    pub fn author(&self, tier: &str, bind: &[&str], append: &str) {
        let path = self.bringup().join("system.toml");
        let mut raw = fs::read_to_string(&path).expect("read system.toml");
        for name in bind {
            let row = format!("name = \"{name}\"");
            assert!(
                raw.contains(&row),
                "the fixture has no `[[component]]` named {name}"
            );
            raw = raw.replace(
                &row,
                &format!("{row}\ngroup_tiers = {{ main = \"{tier}\" }}"),
            );
        }
        fs::write(&path, format!("{raw}\n{append}\n")).expect("author system.toml");
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
/// resolver, written where a bake would write it. The model is a build
/// artifact, so the test makes one. Returns the model's path.
///
/// `with_system` folds the bringup's `system.toml` into the model, which is
/// what carries `[tiers.*]` and `group_tiers` across — the same `--system`
/// `nros sync` passes whenever the file exists (`cmd/ws.rs`). The zero-grammar
/// derived road does not need it: its `system.toml` states no tier at all.
pub fn resolve_model_path(bringup: &Path, with_system: bool) -> PathBuf {
    let resolver = super::pinned_launch_resolver();
    let out = bringup.join("model-out");
    fs::create_dir_all(&out).expect("create the model out dir");
    let model = out.join("system_model.yaml");
    let mut cmd = std::process::Command::new(&resolver);
    cmd.arg(bringup.join("launch/system.launch.xml"))
        .arg("--bringup-root")
        .arg(bringup);
    if with_system {
        cmd.arg("--system").arg(bringup.join("system.toml"));
    }
    let output = cmd
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
    model
}

/// [`resolve_model_path`], parsed.
pub fn resolve_model(bringup: &Path, with_system: bool) -> ros_launch_manifest_model::SystemModel {
    let path = resolve_model_path(bringup, with_system);
    let text = fs::read_to_string(&path).expect("read the resolved model");
    ros_launch_manifest_model::SystemModel::from_yaml_str(&text).expect("the resolved model parses")
}
