//! Phase 211.J — `<include>` recursion safety + depth cap.
//!
//! Drives `nros plan` against `fixtures/orchestration_includes/` with three
//! pre-baked `record-{chain,cycle,deep}.json` files and gates each of
//! 211.J's behaviors:
//!
//! 1. **3-level chain** (`system → level_a → level_b → leaf`) — *works
//!    today*. Gated by `chain_3_levels_resolves_to_leaf`.
//!
//! 2. **Cyclic include** (`cycle_a → cycle_b → cycle_a → …`) — *planner
//!    silently returns an empty plan instead of raising a clean
//!    "cycle detected" error*. `play_launch_parser` swallows the cycle
//!    (dedupes by file path / hits an internal bound), so `record.json`
//!    has zero nodes and the planner emits zero instances. The 211.J
//!    bullet calls for a clean error. Gated by an `#[ignore]`-d test
//!    that flips on once the planner asserts a cycle diagnostic.
//!
//! 3. **17-level depth** (entry → lvl_0 → … → lvl_15 → lvl_16) —
//!    *no depth cap enforced*. 17 nested includes pass through and the
//!    leaf lands; 211.J proposes a default cap of 16. Gated by an
//!    `#[ignore]`-d test for the post-cap behavior.
//!
//! ## Fixture shape
//!
//! Launch files are committed for documentation (the `$(dirname)`
//! placeholders show the topology) but `play_launch_parser` doesn't
//! expand `$(dirname)` and can't resolve in-tree `$(find-pkg-share)`
//! either — so the committed `record-*.json` files (output of the
//! parser after `bake-records.sh` rewrites the placeholders to absolute
//! paths) are the actual test inputs. `--record` makes `nros plan`
//! ignore the launch path entirely, so the tests are portable.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

fn fixture_dir() -> PathBuf {
    nros_tests::project_root().join("packages/testing/nros-tests/fixtures/orchestration_includes")
}

fn plan(record_name: &str, launch_name: &str) -> (std::process::Output, Option<serde_json::Value>) {
    let nros = nros_tests::nros_cli_bin_path().expect("require_nros_cli passed");
    let fixture = fixture_dir();
    let record_path = fixture.join(record_name);
    assert!(
        record_path.is_file(),
        "fixture missing committed {record_name}: {}",
        record_path.display()
    );

    let out = tempfile::tempdir().expect("tempdir");
    let result = Command::new(&nros)
        .arg("plan")
        .arg("demo_inc")
        .arg(format!("demo_inc_bringup/launch/{launch_name}"))
        .arg("--workspace")
        .arg(&fixture)
        .arg("--record")
        .arg(&record_path)
        .arg("--metadata")
        .arg(fixture.join("_metadata/leaf.json"))
        .arg("--out-dir")
        .arg(out.path())
        .output()
        .expect("spawn nros plan");

    let plan_path = out.path().join("nros-plan.json");
    let parsed = if plan_path.is_file() {
        Some(
            serde_json::from_str(&std::fs::read_to_string(&plan_path).expect("read plan"))
                .expect("parse plan"),
        )
    } else {
        None
    };
    (result, parsed)
}

#[test]
fn chain_3_levels_resolves_to_leaf() {
    if !nros_tests::require_nros_cli() {
        nros_tests::skip!("nros CLI not found");
    }
    let (result, plan) = plan("record-chain.json", "system.launch.xml");
    assert!(
        result.status.success(),
        "nros plan exit={} stderr={}",
        result.status,
        String::from_utf8_lossy(&result.stderr)
    );
    let plan = plan.expect("plan json");
    let instances = plan["instances"].as_array().expect("instances array");

    // The leaf node is buried 3 includes deep (system → level_a → level_b
    // → leaf). The planner must walk every include and surface the leaf
    // node, otherwise basic launch composition is broken.
    assert_eq!(
        instances.len(),
        1,
        "expected exactly one leaf instance after 3-level include walk: {instances:#?}"
    );
    let leaf = &instances[0];
    assert_eq!(leaf["component"], "demo_inc::leaf", "wrong component");
    assert_eq!(leaf["launch_name"], "/leaf_node", "wrong launch_name");
}

/// Stage the demo_inc post-212 workspace shape (workspace + per-pkg
/// Cargo.toml, package.xml, bringup `system.toml`, sidecar `_metadata/`)
/// into a tempdir, then write the launch files for the scenario by
/// invoking `write_launches` with the absolute launch dir. Returns the
/// staged workspace root. Used by the cycle + depth-cap tests, which can't
/// use the committed launch files: `play_launch_parser` doesn't expand
/// `$(dirname)`, so each `<include file=…>` needs an absolute path that
/// only exists at test time.
///
/// The staged tree mirrors the post-Phase-212.I migrated fixture:
///   <tmp>/Cargo.toml                      (workspace)
///   <tmp>/demo_inc_bringup/system.toml    (system + components)
///   <tmp>/demo_inc_bringup/package.xml
///   <tmp>/demo_inc_bringup/launch/<test launches written here too>
///   <tmp>/src/demo_inc/Cargo.toml         (per-pkg, with [package.metadata.nros.component])
///   <tmp>/src/demo_inc/package.xml
///   <tmp>/src/demo_inc/launch/<entry written by write_launches>
///   <tmp>/_metadata/leaf.json
fn stage_demo_inc(write_launches: impl FnOnce(&Path)) -> tempfile::TempDir {
    let fixture = fixture_dir();
    let src = fixture.join("src/demo_inc");
    let bringup_src = fixture.join("demo_inc_bringup");
    let metadata_src = fixture.join("_metadata");

    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    let dest_pkg = root.join("src/demo_inc");
    let dest_bringup = root.join("demo_inc_bringup");
    let dest_metadata = root.join("_metadata");

    std::fs::create_dir_all(dest_pkg.join("launch")).unwrap();
    std::fs::create_dir_all(dest_bringup.join("launch")).unwrap();
    std::fs::create_dir_all(&dest_metadata).unwrap();

    // Workspace root Cargo.toml — straight copy from the migrated fixture.
    std::fs::copy(fixture.join("Cargo.toml"), root.join("Cargo.toml")).unwrap();
    // Per-pkg Cargo.toml + package.xml.
    std::fs::copy(src.join("Cargo.toml"), dest_pkg.join("Cargo.toml")).unwrap();
    std::fs::copy(src.join("package.xml"), dest_pkg.join("package.xml")).unwrap();
    // Bringup pkg.
    std::fs::copy(
        bringup_src.join("system.toml"),
        dest_bringup.join("system.toml"),
    )
    .unwrap();
    std::fs::copy(
        bringup_src.join("package.xml"),
        dest_bringup.join("package.xml"),
    )
    .unwrap();
    // _metadata sidecar (preserved across migrate; see fixture README).
    for entry in std::fs::read_dir(&metadata_src).unwrap().flatten() {
        let to = dest_metadata.join(entry.file_name());
        std::fs::copy(entry.path(), to).unwrap();
    }

    // The cycle/depth tests write their entry launch under
    // `src/demo_inc/launch/`; tolerate either layout.
    write_launches(&dest_pkg.join("launch"));
    tmp
}

fn run_plan_live(
    workspace: &Path,
    launch_subpath: &str,
    env: &[(&str, &str)],
) -> std::process::Output {
    let nros = nros_tests::nros_cli_bin_path().expect("require_nros_cli passed");
    let out = tempfile::tempdir().expect("tempdir");
    let mut cmd = Command::new(&nros);
    cmd.arg("plan")
        .arg("demo_inc")
        // Run with the workspace as cwd so `play_launch_parser` resolves the
        // `src/demo_inc/launch/<file>` arg against the live tempdir (the
        // parser interprets relative paths against its own cwd, not against
        // `--workspace`).
        .current_dir(workspace)
        .arg(format!("src/demo_inc/launch/{launch_subpath}"))
        .arg("--workspace")
        .arg(workspace)
        .arg("--out-dir")
        .arg(out.path());
    for (k, v) in env {
        cmd.env(k, v);
    }
    cmd.output().expect("spawn nros plan")
}

fn write(dir: &Path, name: &str, contents: &str) {
    std::fs::write(dir.join(name), contents).unwrap();
}

/// Phase 211.J — cyclic includes raise a clean `CircularInclude` diagnostic.
///
/// Drives `nros plan` against a freshly-written cycle (`cycle_a → cycle_b
/// → cycle_a → …`) with the live parser. Requires the parser binary on
/// PATH; skipped cleanly when missing.
///
/// nros plan always passes `--strict-includes` to the parser (see
/// `nros-cli` planner.rs), so a cycle exits non-zero with the include
/// chain rendered in stderr. Previously this gate was `#[ignore]`-d
/// (parser warn-and-skip default produced an empty plan); flipped on
/// after parser commit 098ccb4 + nros-cli planner commit a2675aa landed.
#[test]
fn cycle_rejected_with_clear_diagnostic() {
    if !nros_tests::require_nros_cli() {
        nros_tests::skip!("nros CLI not found");
    }
    if std::process::Command::new("play_launch_parser")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_err()
    {
        nros_tests::skip!("play_launch_parser not on PATH");
    }

    let tmp = stage_demo_inc(|launch_dir| {
        let a = launch_dir.join("cycle_a.launch.xml");
        let b = launch_dir.join("cycle_b.launch.xml");
        let entry = launch_dir.join("cycle_entry.launch.xml");
        write(
            launch_dir,
            "cycle_entry.launch.xml",
            &format!(
                "<launch>\n  <include file=\"{}\" />\n</launch>\n",
                a.display()
            ),
        );
        write(
            launch_dir,
            "cycle_a.launch.xml",
            &format!(
                "<launch>\n  <include file=\"{}\" />\n</launch>\n",
                b.display()
            ),
        );
        write(
            launch_dir,
            "cycle_b.launch.xml",
            &format!(
                "<launch>\n  <include file=\"{}\" />\n</launch>\n",
                a.display()
            ),
        );
        let _ = entry; // entry only used to confirm path resolves; intentional
    });
    let result = run_plan_live(tmp.path(), "cycle_entry.launch.xml", &[]);
    assert!(
        !result.status.success(),
        "nros plan should reject the cyclic include; exit={}, stdout=\n{}",
        result.status,
        String::from_utf8_lossy(&result.stdout)
    );
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        stderr.contains("Cyclic") || stderr.contains("cycle"),
        "expected a cycle diagnostic in stderr, got:\n{stderr}"
    );
}

/// Phase 211.J — depth-cap enforcement on `<include>` nesting.
///
/// Writes an 18-level chain (entry → lvl_0 → … → lvl_16) and runs
/// `nros plan` with `NROS_PLAY_LAUNCH_MAX_INCLUDE_DEPTH=16` so the parser
/// trips its `MaxIncludeDepthExceeded` guard. Skipped cleanly when the
/// parser binary isn't on PATH.
///
/// Previously `#[ignore]`-d (parser's default cap of 100 wouldn't trip
/// on a 17-level chain); flipped on after parser commit 098ccb4 + nros-cli
/// planner commit a2675aa added the env-var-driven cap.
#[test]
fn depth_cap_rejects_over_16() {
    if !nros_tests::require_nros_cli() {
        nros_tests::skip!("nros CLI not found");
    }
    if std::process::Command::new("play_launch_parser")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_err()
    {
        nros_tests::skip!("play_launch_parser not on PATH");
    }

    let tmp = stage_demo_inc(|launch_dir| {
        // 17 intermediates lvl_0..lvl_16; lvl_16 carries the leaf node.
        for i in 0..=16usize {
            let name = format!("lvl_{i}.launch.xml");
            if i < 16 {
                let next = launch_dir.join(format!("lvl_{}.launch.xml", i + 1));
                write(
                    launch_dir,
                    &name,
                    &format!(
                        "<launch>\n  <include file=\"{}\" />\n</launch>\n",
                        next.display()
                    ),
                );
            } else {
                write(
                    launch_dir,
                    &name,
                    "<launch>\n  <node pkg=\"demo_inc\" exec=\"leaf\" name=\"leaf_node\" />\n</launch>\n",
                );
            }
        }
        let lvl0 = launch_dir.join("lvl_0.launch.xml");
        write(
            launch_dir,
            "deep_entry.launch.xml",
            &format!(
                "<launch>\n  <include file=\"{}\" />\n</launch>\n",
                lvl0.display()
            ),
        );
    });
    let result = run_plan_live(
        tmp.path(),
        "deep_entry.launch.xml",
        &[("NROS_PLAY_LAUNCH_MAX_INCLUDE_DEPTH", "16")],
    );
    assert!(
        !result.status.success(),
        "nros plan should reject the over-cap include chain; exit={}, stdout=\n{}",
        result.status,
        String::from_utf8_lossy(&result.stdout)
    );
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        stderr.contains("Maximum include depth")
            || stderr.contains("depth")
            || stderr.contains("16"),
        "expected a depth-cap diagnostic in stderr, got:\n{stderr}"
    );
}
