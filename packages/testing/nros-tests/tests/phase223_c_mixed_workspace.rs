//! Phase 223 — C Node pkg workspace coverage.
//!
//! This verifies the mixed C/C++ reference template and the pure-C
//! reference template configure and build. It intentionally does not
//! assert publish/subscribe traffic: the runtime instantiator for
//! recorded C/C++ NodeEntityDescriptor values is tracked outside Phase 223.

use std::{
    path::PathBuf,
    process::{Command, Stdio},
};

fn workspace_root() -> PathBuf {
    nros_tests::project_root()
}

fn require_prereqs() -> bool {
    nros_tests::require_nros_cli() && nros_tests::process::require_cmake()
}

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
fn c_node_pkg_links_into_cpp_entry_template() {
    build_template("c-and-cpp-mixed-workspace");
}

#[test]
fn c_node_pkgs_link_into_c_entry_template() {
    build_template("pure-c-workspace");
}

fn build_template(template: &str) {
    if !require_prereqs() {
        nros_tests::skip!("prereqs missing (nros CLI / cmake)");
    }
    if !play_launch_parser_available() {
        nros_tests::skip!(
            "play_launch_parser not on PATH (pip install play-launch-parser, or build its binary)"
        );
    }

    let root = workspace_root();
    let source = root.join("examples/templates").join(template);
    let build = tempfile::tempdir().expect("build tempdir");

    let mut configure_cmd = Command::new("cmake");
    configure_cmd
        .args(["-S"])
        .arg(&source)
        .arg("-B")
        .arg(build.path());
    if let Some(nros) = nros_tests::nros_cli_bin_path() {
        configure_cmd.arg(format!("-DNROS_BIN={}", nros.display()));
    }
    let configure = configure_cmd
        .current_dir(&root)
        .output()
        .expect("spawn cmake configure");
    assert!(
        configure.status.success(),
        "cmake configure failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&configure.stdout),
        String::from_utf8_lossy(&configure.stderr)
    );

    let build_out = Command::new("cmake")
        .arg("--build")
        .arg(build.path())
        .current_dir(&root)
        .output()
        .expect("spawn cmake build");
    assert!(
        build_out.status.success(),
        "cmake build failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build_out.stdout),
        String::from_utf8_lossy(&build_out.stderr)
    );

    let exe = build.path().join("src/robot_entry/robot_entry");
    assert!(
        exe.is_file(),
        "{template}: missing Entry pkg binary at {}",
        exe.display()
    );
}
