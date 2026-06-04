//! Phase 219.acc — C++ multi-node-workspace Entry-pkg cmake fixture.
//!
//! Asserts the §6 acceptance bar's structural floor for the C++
//! Entry-pkg path: configure + build the in-tree
//! `examples/templates/multi-node-workspace-cpp/` template (with the
//! `src/robot_entry/` Entry pkg landed in 219.acc) against the host
//! toolchain + the host `nros` CLI, then verify
//!
//!   * `nros codegen entry --lang cpp` produced the expected
//!     generated TU under `${CMAKE_BINARY_DIR}/src/robot_entry/`,
//!     declaring + calling both Node-pkg register fns in launch order;
//!   * Phase 219.J auto-link sidecar (`robot_entry_link_libs.cmake`)
//!     names both Node-pkg static libs;
//!   * the build linked an exe at
//!     `<build>/src/robot_entry/robot_entry`.
//!
//! Runtime (talker publishes 0,1,2,… ; listener receives) is NOT
//! checked here — the Native NodeContext runtime that would turn
//! recorded entities into running pubs/subs sits below the Phase 219
//! orchestration scope (phase doc §7). Per the workflow review the
//! runtime bar is a separate follow-up; this test guards the
//! build-system + codegen plumbing that every other 219 fixture
//! inherits.
//!
//! Test FAILS (not skips) when cmake / a C++ compiler are absent —
//! same "no silent skip" rule the cmake_add_subdirectory smoke test
//! follows. Skip-via-panic (`nros_tests::skip!`) only when the host
//! `nros` build tool isn't installed.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

fn workspace_root() -> PathBuf {
    // Walk up from `CARGO_MANIFEST_DIR` (= `packages/testing/nros-tests`)
    // to the nano-ros root.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent() // packages/testing/
        .and_then(Path::parent) // packages/
        .and_then(Path::parent) // <root>
        .expect("workspace root")
        .to_path_buf()
}

/// Resolve a `nros` CLI binary path the cmake fn can shell.
///
/// Priority matches `cmake/NanoRosEntry.cmake`'s `_nros_entry_invoke_codegen`
/// helper: `NROS_CLI` env → PATH → `~/.nros/bin/nros`. Returns `None`
/// when nothing resolves (caller emits a skip with a hint).
fn resolve_nros_bin() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("NROS_CLI") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    if Command::new("nros")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
    {
        // `nros` is on PATH; cmake will resolve it via `find_program`.
        // The `which` crate isn't a dep — return a sentinel that the
        // test can ignore (cmake fn handles PATH discovery on its own).
        return Some(PathBuf::from("nros"));
    }
    let home = std::env::var_os("NROS_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join(".nros")));
    let candidate = home?.join("bin/nros");
    if candidate.is_file() {
        Some(candidate)
    } else {
        None
    }
}

#[test]
fn multi_node_workspace_cpp_configures_and_builds() {
    if !Command::new("cmake")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
    {
        panic!("cmake not available on PATH; install cmake to run this test");
    }

    // The `nros` CLI must support the new `codegen entry` subcommand;
    // the installed `~/.nros/bin/nros` (Phase 195.D prebuilt) predates
    // 219.A. Prefer an explicit `NROS_CLI` override pointing at the
    // in-tree binary built by `cargo build -p nros-cli --bin nros`.
    let Some(nros_bin) = resolve_nros_bin() else {
        nros_tests::skip!(
            "no `nros` CLI binary resolved (tried $NROS_CLI / PATH / ~/.nros/bin); \
             run `scripts/install-nros.sh` or set NROS_CLI to the in-tree \
             `packages/cli/target/<profile>/nros` so this test exercises the \
             Phase 219 `codegen entry` subcommand"
        );
    };

    // Check the resolved binary actually supports `codegen entry`.
    let probe = Command::new(&nros_bin)
        .args(["codegen", "entry", "--help"])
        .output();
    let codegen_entry_ok = probe.is_ok_and(|o| o.status.success());
    if !codegen_entry_ok {
        nros_tests::skip!(
            "resolved `nros` CLI at `{}` does not support `codegen entry` \
             (Phase 219.A); set NROS_CLI to a build from this branch",
            nros_bin.display()
        );
    }

    let root = workspace_root();
    let src = root.join("examples/templates/multi-node-workspace-cpp");
    assert!(
        src.join("src/robot_entry/CMakeLists.txt").is_file(),
        "fixture missing: src/robot_entry was not landed by Phase 219.acc"
    );

    let tmp = tempfile::Builder::new()
        .prefix("nros-219-cpp-")
        .tempdir()
        .expect("tempdir");
    let build = tmp.path();

    // Configure.
    let mut cfg = Command::new("cmake");
    cfg.arg("-S")
        .arg(&src)
        .arg("-B")
        .arg(build)
        .arg(format!("-DNROS_CLI_BIN={}", nros_bin.display()));
    let cfg_out = cfg.output().expect("run cmake configure");
    assert!(
        cfg_out.status.success(),
        "cmake configure failed:\n--- stdout ---\n{}\n--- stderr ---\n{}",
        String::from_utf8_lossy(&cfg_out.stdout),
        String::from_utf8_lossy(&cfg_out.stderr)
    );

    // The generated TU + sidecar should exist after configure.
    let gen_tu = build.join("src/robot_entry/robot_entry_nros_main_generated.cpp");
    let link_libs = build.join("src/robot_entry/robot_entry_link_libs.cmake");
    let depfile = build.join("src/robot_entry/robot_entry_nros_main_generated.d");
    assert!(
        gen_tu.is_file(),
        "missing generated TU at `{}`",
        gen_tu.display()
    );
    assert!(
        link_libs.is_file(),
        "missing link-libs sidecar at `{}`",
        link_libs.display()
    );
    assert!(
        depfile.is_file(),
        "missing depfile at `{}`",
        depfile.display()
    );

    let gen_body = std::fs::read_to_string(&gen_tu).expect("read generated TU");
    assert!(
        gen_body.contains("__nros_component_talker_pkg_register"),
        "generated TU missing talker_pkg register call:\n{gen_body}"
    );
    assert!(
        gen_body.contains("__nros_component_listener_pkg_register"),
        "generated TU missing listener_pkg register call:\n{gen_body}"
    );
    // Launch-XML order: talker before listener.
    let pos_t = gen_body
        .find("__nros_component_talker_pkg_register(context)")
        .expect("talker call");
    let pos_l = gen_body
        .find("__nros_component_listener_pkg_register(context)")
        .expect("listener call");
    assert!(
        pos_t < pos_l,
        "register call order doesn't match launch XML"
    );

    let link_body = std::fs::read_to_string(&link_libs).expect("read link sidecar");
    assert!(link_body.contains("talker_pkg_talker_component"));
    assert!(link_body.contains("listener_pkg_listener_component"));

    let dep_body = std::fs::read_to_string(&depfile).expect("read depfile");
    assert!(
        dep_body.contains("system.launch.xml"),
        "depfile missing launch.xml entry:\n{dep_body}"
    );
    assert!(
        dep_body.contains("package.xml"),
        "depfile missing package.xml entries:\n{dep_body}"
    );

    // Build.
    let mut blk = Command::new("cmake");
    blk.arg("--build").arg(build).arg("-j");
    let blk_out = blk.output().expect("run cmake build");
    assert!(
        blk_out.status.success(),
        "cmake build failed:\n--- stdout ---\n{}\n--- stderr ---\n{}",
        String::from_utf8_lossy(&blk_out.stdout),
        String::from_utf8_lossy(&blk_out.stderr)
    );

    let exe = build.join("src/robot_entry/robot_entry");
    assert!(
        exe.is_file(),
        "robot_entry executable missing at `{}`",
        exe.display()
    );
}
