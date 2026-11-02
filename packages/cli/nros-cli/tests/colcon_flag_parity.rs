//! `nros build` and `nros list` select the packages `colcon list` selects,
//! given the same flags, over `testing_workspaces/ros2_rust_examples`.
//!
//! RFC-0117 D4 takes colcon's flag names only where the meaning is the same;
//! this file is what holds the meaning to colcon's (phase-526 W3). Each case
//! runs one flag set three ways — `colcon list -n`, `nros list -n` and
//! `nros build --dry-run` — and asserts one package set.
//!
//! `#[ignore]`d, because it needs `colcon` on PATH and the
//! `ros2_rust_examples` submodule checked out, neither of which a plain
//! `cargo test` can assume. `just check cli-tests` runs it with `--ignored`
//! where both are present and records it as unverified where they are not.
//! Run with `--ignored`, a missing precondition FAILS — it never passes over
//! nothing.
//!
//! Only CLOSED selections are compared: a selection that leaves out a package
//! a selected one depends on is one colcon lists and `nros` refuses, on
//! purpose (`builder::discover::select`), and the last test pins that.

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    process::Command,
};

fn workspace_src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../testing_workspaces/ros2_rust_examples")
        .canonicalize()
        .expect("testing_workspaces/ros2_rust_examples")
}

/// The preconditions, asserted: `colcon` and the populated submodule.
fn preconditions() {
    let colcon = Command::new("colcon").arg("--help").output();
    assert!(
        colcon.as_ref().is_ok_and(|o| o.status.success()),
        "colcon is not on PATH ({colcon:?}); install `colcon-common-extensions` \
         (`nros setup` provisions it) or run without --ignored"
    );
    let rclrs = workspace_src().join("rclrs");
    assert!(
        rclrs.join("message_demo/package.xml").is_file(),
        "{} is empty: run `git submodule update --init \
         packages/cli/testing_workspaces/ros2_rust_examples`",
        rclrs.display()
    );
}

/// A private copy of the workspace: `nros build --dry-run` still generates,
/// and neither tool may write into the submodule.
fn copy_workspace() -> tempfile::TempDir {
    fn copy(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).unwrap();
        for e in std::fs::read_dir(from).unwrap().flatten() {
            let name = e.file_name();
            if name == ".git" || name == "target" || name == "build" {
                continue;
            }
            let (src, dst) = (e.path(), to.join(&name));
            if e.file_type().unwrap().is_dir() {
                copy(&src, &dst);
            } else {
                std::fs::copy(&src, &dst).unwrap();
            }
        }
    }
    let tmp = tempfile::tempdir().unwrap();
    copy(&workspace_src().join("rclrs"), &tmp.path().join("rclrs"));
    tmp
}

fn lines(out: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(out)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}

fn colcon_list(ws: &Path, extra: &[&str]) -> Vec<String> {
    let out = Command::new("colcon")
        .arg("list")
        .args(extra)
        .current_dir(ws)
        .output()
        .expect("spawn colcon");
    assert!(
        out.status.success(),
        "colcon list {extra:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    lines(&out.stdout)
}

fn nros() -> Command {
    Command::new(env!("CARGO_BIN_EXE_nros"))
}

fn nros_list(ws: &Path, extra: &[&str]) -> Vec<String> {
    let out = nros()
        .arg("list")
        .arg(ws)
        .args(extra)
        .output()
        .expect("spawn nros");
    assert!(
        out.status.success(),
        "nros list {extra:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    lines(&out.stdout)
}

/// What `nros build --dry-run` would build: one announcement line per
/// package, plus one line per interface package it generates into its users.
fn nros_build_set(ws: &Path, flags: &[&str]) -> BTreeSet<String> {
    let out = nros()
        .args(["build", "--dry-run", "--workspace"])
        .arg(ws)
        .args(flags)
        // `rclrs` and `rosidl_runtime_rs` are upstream ROS 2 Rust packages,
        // which nothing here provides; the selection is what is under test.
        .env("NROS_ALLOW_UNRESOLVED_DEPS", "1")
        .output()
        .expect("spawn nros");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "nros build --dry-run {flags:?}: {stderr}"
    );
    let mut set = BTreeSet::new();
    for l in stderr.lines() {
        let Some(rest) = l.strip_prefix("nros build: ") else {
            continue;
        };
        // `[3/7] pkg -> board …`, or `pkg -> board …` for a single plan.
        let rest = match rest.strip_prefix('[') {
            Some(r) => r.split_once("] ").map_or(r, |(_, t)| t),
            None => rest,
        };
        if let Some((name, _)) = rest.split_once(" -> board ") {
            set.insert(name.to_string());
        } else if let Some((name, _)) = rest.split_once(" — interface package") {
            set.insert(name.to_string());
        }
    }
    set
}

/// The flag sets compared. Each is closed under `<depend>`, so colcon's answer
/// and ours must agree exactly.
const CASES: &[&[&str]] = &[
    &[],
    &["--packages-select", "rust_pubsub"],
    &[
        "--packages-select",
        "rclrs_example_msgs",
        "examples_rclrs_message_demo",
        "examples_worker_demo",
    ],
    &["--packages-up-to", "examples_rclrs_message_demo"],
    &[
        "--packages-up-to",
        "examples_rclrs_message_demo",
        "rust_pubsub",
    ],
    &["--packages-above", "rclrs_example_msgs"],
    &["--packages-skip", "rust_pubsub", "examples_worker_demo"],
    &[
        "--base-paths",
        "rclrs/message_demo",
        "rclrs/rclrs_example_msgs",
        "rclrs/rust_pubsub",
    ],
    // Composition: each flag deselects, so the result is the intersection.
    &[
        "--packages-up-to",
        "examples_rclrs_message_demo",
        "rust_pubsub",
        "--packages-skip",
        "rust_pubsub",
    ],
    &[
        "--packages-above",
        "rclrs_example_msgs",
        "--packages-select",
        "rclrs_example_msgs",
        "examples_rclrs_message_demo",
        "rust_pubsub",
    ],
    &[
        "--base-paths",
        "rclrs",
        "--packages-skip",
        "examples_logging_demo",
    ],
];

#[test]
#[ignore = "needs colcon and the ros2_rust_examples submodule; `just check cli-tests` runs it"]
fn selection_flags_select_what_colcon_selects() {
    preconditions();
    let colcon_ws = copy_workspace();
    let nros_ws = copy_workspace();
    for flags in CASES {
        let mut with_n = vec!["-n"];
        with_n.extend_from_slice(flags);
        let want: BTreeSet<String> = colcon_list(colcon_ws.path(), &with_n).into_iter().collect();
        assert!(!want.is_empty(), "colcon selected nothing for {flags:?}");
        let listed: BTreeSet<String> = nros_list(nros_ws.path(), &with_n).into_iter().collect();
        assert_eq!(listed, want, "nros list -n {flags:?} vs colcon list -n");
        let built = nros_build_set(nros_ws.path(), flags);
        assert_eq!(
            built, want,
            "nros build --dry-run {flags:?} vs colcon list -n"
        );
    }
}

#[test]
#[ignore = "needs colcon and the ros2_rust_examples submodule; `just check cli-tests` runs it"]
fn list_output_flags_match_colcon() {
    preconditions();
    let colcon_ws = copy_workspace();
    let nros_ws = copy_workspace();

    // `-n`, no order flag: both sort by name, so the lines are equal.
    assert_eq!(
        nros_list(nros_ws.path(), &["-n"]),
        colcon_list(colcon_ws.path(), &["-n"]),
        "-n"
    );
    // `-p`: the same paths, relative to the workspace, in path order.
    assert_eq!(
        nros_list(nros_ws.path(), &["-p"]),
        colcon_list(colcon_ws.path(), &["-p"]),
        "-p"
    );
    // The default line is `name<TAB>path<TAB>(type)`; the name and path
    // columns agree, the type column is each tool's own vocabulary.
    let cols = |l: &String| {
        let mut it = l.split('\t');
        (
            it.next().unwrap_or_default().to_string(),
            it.next().unwrap_or_default().to_string(),
        )
    };
    let ours: Vec<_> = nros_list(nros_ws.path(), &["--packages-skip", "rust_pubsub"])
        .iter()
        .map(cols)
        .collect();
    let theirs: Vec<_> = colcon_list(colcon_ws.path(), &["--packages-skip", "rust_pubsub"])
        .iter()
        .map(cols)
        .collect();
    assert_eq!(ours, theirs, "name and path columns");

    // `-t`: the same set, and a dependency before its dependent. Ties between
    // unrelated packages are each tool's own; only the edges are the contract.
    let ours = nros_list(nros_ws.path(), &["-t", "-n"]);
    let theirs = colcon_list(colcon_ws.path(), &["-t", "-n"]);
    assert_eq!(
        ours.iter().collect::<BTreeSet<_>>(),
        theirs.iter().collect::<BTreeSet<_>>(),
        "-t set"
    );
    let pos = |n: &str| ours.iter().position(|l| l == n).expect(n);
    assert!(
        pos("rclrs_example_msgs") < pos("examples_rclrs_message_demo"),
        "-t puts the interface package before its user: {ours:?}"
    );
}

#[test]
#[ignore = "needs colcon and the ros2_rust_examples submodule; `just check cli-tests` runs it"]
fn an_open_selection_colcon_lists_is_refused() {
    // The documented divergence: colcon lists the package alone and builds it
    // against an install prefix; nano-ros has none, so it refuses and names
    // the missing dependency.
    preconditions();
    let colcon_ws = copy_workspace();
    let nros_ws = copy_workspace();
    let flags = ["-n", "--packages-select", "examples_rclrs_message_demo"];
    assert_eq!(
        colcon_list(colcon_ws.path(), &flags),
        ["examples_rclrs_message_demo"]
    );
    let out = nros()
        .arg("list")
        .arg(nros_ws.path())
        .args(flags)
        .output()
        .expect("spawn nros");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "must refuse: {stderr}");
    assert!(
        stderr.contains("examples_rclrs_message_demo needs rclrs_example_msgs"),
        "{stderr}"
    );
}
