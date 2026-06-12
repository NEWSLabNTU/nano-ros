//! Phase 212.O.5 — `n11_launch_xml_ros2_compat_smoke`.
//!
//! Stages the `o5_nav2_compat_smoke` fixture into a tempdir + rewrites
//! `@NANO_ROS_ROOT@` to absolute `path =` deps, then runs
//! `cargo build -p demo_entry` against it. The fixture's primary
//! `launch/system.launch.xml` is a nav2-style copy-paste — `<arg>`,
//! `<node>` with `pkg=`/`exec=`/`name=`/`namespace=`, `<param
//! value="$(var …)"/>`, `<remap from= to=>`, plus an `<include
//! file="$(find secondary_node)/…"/>` that pulls in a sibling launch.xml
//! shipped inside the `secondary_node` package.
//!
//! Asserts (when the codegen path runs):
//!
//! 1. `cargo build -p demo_entry` exits zero.
//! 2. The emitted `$OUT_DIR/run_plan.rs` references BOTH
//!    `primary_node::register` AND `secondary_node::register` — proof
//!    the `<include>` sub-tree was expanded and the `$(find …)`
//!    substitution resolved to the secondary pkg's launch dir.
//! 3. The sibling `$OUT_DIR/nros-system/nros-plan.json` carries (a)
//!    the `<arg>`-default `robot1` (propagated via `$(var namespace)`
//!    into the primary's `robot_namespace` param), and (b) the
//!    `primary/chatter` remap target. Both are evidence the planner
//!    honoured the nav2-style directives end-to-end.
//!
//! Same offline-CI gating as `phase212_h3_freertos`: the fixture's
//! `build.rs` falls back to a placeholder stub when the git-based
//! `nros-build` dep cannot fetch (offline / no network), and when
//! `play_launch_parser` is missing the planner errors before emitting
//! a plan. Both shapes are reported via `nros_tests::skip!` rather
//! than passed silently — see CLAUDE.md "Tests must fail on unmet
//! preconditions."
//!
//! Marked `#[ignore]` while Phase 212.O.5 is the gate for confirming
//! the N.11 surface lands end-to-end in nano-ros — the M-F.17
//! component-metadata α-bridge + N.11 launch parser ship inside the
//! pinned `nros-cli` (Phase 212.M-F.17 / N.11 landed nros-cli
//! `6b69d6e` 2026-06-03), but neither the H.3 nor the L.7 sister
//! tests promote the inspect-the-emitted-plan path beyond the
//! Placeholder fallback today. Lift the ignore once a green CI run
//! has exercised the full path (covered by the O.5 work item).
//!
//! Run with: `cargo test -p nros-tests --test phase212_o5_nav2_compat -- --ignored`

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

fn workspace_root() -> PathBuf {
    nros_tests::project_root()
}

fn fixture_src() -> PathBuf {
    workspace_root().join("packages/testing/nros-tests/fixtures/o5_nav2_compat_smoke")
}

fn stage_fixture() -> (tempfile::TempDir, PathBuf) {
    let src = fixture_src();
    let dst = tempfile::tempdir().expect("tempdir");
    copy_tree(&src, dst.path()).expect("copy fixture");
    let root_str = workspace_root()
        .to_str()
        .expect("workspace root utf-8")
        .to_string();
    rewrite_placeholders(dst.path(), &root_str).expect("rewrite placeholders");
    let root = dst.path().to_path_buf();
    (dst, root)
}

fn copy_tree(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_tree(&from, &to)?;
        } else if ty.is_file() {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

fn rewrite_placeholders(root: &Path, replacement: &str) -> std::io::Result<()> {
    // Phase 212.O.5 — resolve `@NROS_CLI_ROOT@` for the offline-friendly
    // nros-build patch-override (see fixture demo_entry/Cargo.toml).
    // Post-Phase-218 the CLI lives in-tree at `packages/cli/`; default
    // there. Fall back to a sibling `nros-cli/` checkout for users still
    // on the pre-218 layout.
    // Resolve the `nros-build` crate dir across layouts — in-tree
    // `packages/cli/nros-build`, sibling `../nros-cli/packages/nros-build`,
    // or `$NROS_CLI_ROOT/{nros-build,packages/nros-build}` — then substitute
    // its PARENT for `@NROS_CLI_ROOT@` so the fixture's
    // `@NROS_CLI_ROOT@/nros-build` patch path resolves regardless of layout
    // (the in-tree `packages/cli/` drops the external repo's `packages/`
    // segment).
    let find_nros_build = |base: &std::path::Path| -> Option<std::path::PathBuf> {
        ["nros-build", "packages/nros-build"]
            .into_iter()
            .map(|sub| base.join(sub))
            .find(|cand| cand.join("Cargo.toml").is_file())
    };
    let nros_cli_root = std::env::var("NROS_CLI_ROOT")
        .ok()
        .and_then(|p| find_nros_build(std::path::Path::new(&p)))
        .or_else(|| find_nros_build(&std::path::Path::new(replacement).join("packages/cli")))
        .or_else(|| {
            std::path::Path::new(replacement)
                .parent()
                .and_then(|p| find_nros_build(&p.join("nros-cli")))
        })
        .as_deref()
        .and_then(|d| d.parent())
        .map(|p| p.display().to_string());
    for entry in walk(root)? {
        if !entry.is_file() {
            continue;
        }
        let Ok(text) = fs::read_to_string(&entry) else {
            continue;
        };
        let mut new_text = text.replace("@NANO_ROS_ROOT@", replacement);
        if let Some(cli_root) = nros_cli_root.as_deref() {
            new_text = new_text.replace("@NROS_CLI_ROOT@", cli_root);
        }
        if new_text != text {
            fs::write(&entry, new_text)?;
        }
    }
    Ok(())
}

fn walk(root: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(p) = stack.pop() {
        if p.is_dir() {
            for e in fs::read_dir(&p)? {
                stack.push(e?.path());
            }
        } else {
            out.push(p);
        }
    }
    Ok(out)
}

/// Probe the external `play_launch_parser` binary the planner shells
/// out to. The nros-cli planner cannot resolve a launch.xml without
/// it; absence is a hard skip (mirrors `phase212_l7_self_bringup`).
fn play_launch_parser_available() -> bool {
    Command::new("play_launch_parser")
        .arg("--help")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[test]
fn n11_launch_xml_ros2_compat_smoke() {
    if !play_launch_parser_available() {
        nros_tests::skip!(
            "play_launch_parser not on PATH (pip install play-launch-parser, or build its binary) \
             — the nros-cli planner shells out to it to walk the launch graph"
        );
    }

    let (_guard, root) = stage_fixture();
    let demo_entry = root.join("demo_entry");

    // Build host-target `demo_entry` — no embedded toolchain needed.
    let build = Command::new("cargo")
        .args(["build", "-p", "demo_entry"])
        .current_dir(&demo_entry)
        .output()
        .expect("spawn cargo build");

    assert!(
        build.status.success(),
        "cargo build -p demo_entry failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr),
    );

    // Locate the emitted run_plan.rs under demo_entry/target/.../build/
    // demo_entry-<hash>/out/. Walk the tree — the hash is build-specific.
    let target_dir = demo_entry.join("target/debug/build");
    let mut found_run_plan: Option<PathBuf> = None;
    if target_dir.is_dir() {
        for e in walk(&target_dir).unwrap_or_default() {
            if e.file_name().and_then(|n| n.to_str()) == Some("run_plan.rs") {
                found_run_plan = Some(e);
                break;
            }
        }
    }
    let run_plan_path =
        found_run_plan.expect("nros-build did not emit run_plan.rs under demo_entry/target");
    let run_plan = fs::read_to_string(&run_plan_path).expect("read run_plan.rs");

    // Offline-CI gate: when the git-based `nros-build` dep can't fetch
    // (or the planner errored), the fixture build.rs writes a
    // placeholder stub. That keeps the bin compiling but does NOT
    // exercise the N.11 directives — skip with the explicit reason.
    if run_plan.contains("Placeholder") {
        nros_tests::skip!(
            "nros-build codegen path returned the offline-fallback Placeholder stub at {}; \
             no codegen evidence to assert against. Likely causes: the `nros-build` git \
             dep (archived github.com/NEWSLabNTU/nros-cli — post-Phase-218 migrate to \
             the in-tree `packages/cli/` path-patch) failed to resolve, or the planner \
             rejected the launch.xml.",
            run_plan_path.display()
        );
    }

    // Codegen path ran — every `<node>` in the launch graph must
    // show up as a `<pkg>::register(runtime)` call in the emitted body.
    for pkg in ["primary_node", "secondary_node"] {
        let expected = format!("{pkg}::register");
        assert!(
            run_plan.contains(&expected),
            "run_plan.rs missing `{expected}` — directive not honoured:\n{run_plan}"
        );
    }
    assert!(
        run_plan.contains("pub fn run_plan"),
        "run_plan.rs missing `pub fn run_plan` declaration:\n{run_plan}"
    );

    // Sibling plan json — the planner writes parameters / remaps onto
    // `PlanInstance`. The emitted run_plan body collapses those into
    // register calls (no per-instance state in the body yet — see
    // `nros-build::emit`), so the plan json is where we look for
    // `<arg>` propagation, `$(find …)` resolution evidence, and
    // `<remap>` evidence.
    let plan_json_path = run_plan_path
        .parent()
        .expect("run_plan.rs parent dir")
        .join("nros-system/nros-plan.json");
    assert!(
        plan_json_path.is_file(),
        "nros-plan.json missing at {} (codegen emitted run_plan.rs but not the plan json?)",
        plan_json_path.display()
    );
    let plan_json = fs::read_to_string(&plan_json_path).expect("read nros-plan.json");

    // `<arg name="namespace" default="robot1"/>` propagates through
    // `<param value="$(var namespace)"/>` on the primary node →
    // `PlanInstance.parameters` carries the resolved value `robot1`.
    assert!(
        plan_json.contains("robot1"),
        "nros-plan.json missing the `<arg>`-propagated default `robot1` — \
         `<param value=\"$(var namespace)\"/>` did not resolve:\n{plan_json}"
    );

    // `<remap from=\"chatter\" to=\"primary/chatter\"/>` lands on
    // `PlanInstance.remaps` — the rewritten `to` value is the
    // load-bearing string here.
    assert!(
        plan_json.contains("primary/chatter"),
        "nros-plan.json missing the `<remap>` target `primary/chatter`:\n{plan_json}"
    );

    // `<include file=\"$(find secondary_node)/launch/secondary.launch.xml\"/>`
    // resolves through the launch_parser's pkg-index to the staged
    // `secondary_node` directory. The planner records the resolved
    // launch_name on each `PlanInstance` derived from the include.
    assert!(
        plan_json.contains("secondary_node") && plan_json.contains("primary_node"),
        "nros-plan.json missing one of the package references — `<include>` sub-tree \
         was not expanded by `$(find …)`:\n{plan_json}"
    );
}
