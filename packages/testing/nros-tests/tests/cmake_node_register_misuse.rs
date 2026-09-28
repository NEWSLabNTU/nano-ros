//! §212.L.9 cmake-fn reject diagnostics — the configure MUST fail.
//!
//! **Runs cmake at run time — the documented exception to "No compilation
//! inside tests" (AGENTS.md / issue 0041):** a configure-*fail* with a specific
//! diagnostic can't be prebuilt as a passing fixture. The cmake configures fail
//! fast (the cmake fn raises FATAL_ERROR before any compile), so these are not
//! the timeout class; the positive metadata cases moved to build-stage fixtures
//! (`cmake_node_register_metadata.rs`).

use std::{fs, path::PathBuf, process::Command};

fn cmake_module_path() -> PathBuf {
    nros_tests::project_root().join("cmake/NanoRosNodeRegister.cmake")
}

/// Stage a fresh dir with a CMakeLists invoking the cmake fn `body`, plus dummy
/// sources. Returns (guard, root, build_dir).
fn stage(cmake_body: &str, project_name: &str) -> (tempfile::TempDir, PathBuf, PathBuf) {
    let guard = tempfile::tempdir().expect("tempdir");
    let root = guard.path().to_path_buf();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("src/dummy.cpp"),
        "int phase212_l9_stub() { return 0; }\n",
    )
    .unwrap();
    // Plain C stub — these tests FATAL at configure (CLASS mismatch / embedded
    // DEPLOY) before any compile, so the source body is irrelevant; keep it free
    // of the retired declarative seam (phase-257 Stage-3b).
    fs::write(
        root.join("src/dummy.c"),
        "int phase212_l9_stub_c(void) { return 0; }\n",
    )
    .unwrap();
    let cml = format!(
        "cmake_minimum_required(VERSION 3.22)\nproject({project_name} C CXX)\ninclude(\"{module}\")\n{cmake_body}\n",
        module = cmake_module_path().display(),
    );
    fs::write(root.join("CMakeLists.txt"), cml).unwrap();
    let build = root.join("build");
    (guard, root, build)
}

fn configure(root: &PathBuf, build: &PathBuf) -> std::process::Output {
    Command::new("cmake")
        .args(["-S", "."])
        .arg("-B")
        .arg(build)
        .current_dir(root)
        .output()
        .expect("spawn cmake configure")
}

#[test]
fn nano_ros_node_register_rejects_unqualified_class() {
    // RFC-0057 D2 retired the 212.L.4 pkg-prefix rule (CLASS may carry any
    // upstream namespace); the live rule is that CLASS must still be a
    // namespace-QUALIFIED name — the entry codegen needs a real type name.
    if !nros_tests::process::require_cmake() {
        nros_tests::skip!("cmake not on PATH");
    }
    let body = "nano_ros_node_register(\n  NAME talker\n  CLASS Talker\n  SOURCES src/dummy.cpp\n  DEPLOY native)\n";
    let (_g, root, build) = stage(body, "talker_pkg");
    let out = configure(&root, &build);
    assert!(
        !out.status.success(),
        "expected cmake configure to fail on an unqualified CLASS"
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("must be a") && err.contains("namespace-qualified"),
        "expected the RFC-0057 qualified-class diagnostic, got:\n{err}"
    );
}

#[test]
fn nano_ros_application_rejects_embedded_deploy() {
    if !nros_tests::process::require_cmake() {
        nros_tests::skip!("cmake not on PATH");
    }
    // `nano_ros_application` (the 212.N.6 shim) was retired in 287-W8; the
    // live spelling of the same misuse is `nano_ros_entry`.
    let body = "nano_ros_entry(\n  NAME my_app\n  SOURCES src/dummy.cpp\n  DEPLOY native zephyr)\n";
    let (_g, root, build) = stage(body, "my_app");
    let out = configure(&root, &build);
    assert!(
        !out.status.success(),
        "expected cmake configure to fail on embedded DEPLOY in Application"
    );
    let err = String::from_utf8_lossy(&out.stderr);
    // `nano_ros_application` is now a deprecated shim → `nano_ros_entry`; accept
    // the entry-layer board-centric wording or the legacy L.2 wording.
    assert!(
        err.contains("native-only")
            || err.contains("Phase 212.L.2")
            || err.contains("embedded Entry pkgs need a Board")
            || err.contains("rejected"),
        "expected an embedded-deploy rejection diagnostic, got:\n{err}"
    );
}

/// `ENTITIES` is out of the GRAMMAR (retired phase-412; removed from every
/// `cmake_parse_arguments` keyword list in the wave that added this test), so
/// the refusal reads `ARGN` instead of a parse result.
///
/// That is what makes the three positions below one test rather than three
/// unrelated ones: with no keyword in the grammar, `ENTITIES` written after a
/// multi-value keyword is SWALLOWED into it (`SOURCES src/dummy.cpp ENTITIES
/// sub:…` would compile a source file named `ENTITIES`), and written anywhere
/// else it lands in `_NRC_UNPARSED_ARGUMENTS`, which nothing reads — a silent
/// drop of a declaration the caller believes is sizing its pools. Both must
/// refuse, and an `IN_LIST ARGN` test is the only thing that catches both.
#[test]
fn entities_is_refused_wherever_a_stale_caller_writes_it() {
    if !nros_tests::process::require_cmake() {
        nros_tests::skip!("cmake not on PATH");
    }
    for (case, entities_arg) in [
        // swallowed into SOURCES if nothing refuses
        (
            "after a multi-value keyword",
            "SOURCES src/dummy.cpp\n  ENTITIES sub:std_msgs/msg/String:/chatter",
        ),
        // lands in UNPARSED_ARGUMENTS, which nothing reads
        (
            "before any multi-value keyword",
            "ENTITIES sub:std_msgs/msg/String:/chatter\n  SOURCES src/dummy.cpp",
        ),
        // the valueless form the old KEYWORDS_MISSING_VALUES arm covered
        ("valueless", "ENTITIES\n  SOURCES src/dummy.cpp"),
    ] {
        let body = format!(
            "nano_ros_node_register(\n  NAME talker\n  CLASS demo::Talker\n  {entities_arg}\n  DEPLOY native)\n"
        );
        let (_g, root, build) = stage(&body, "talker_pkg");
        let out = configure(&root, &build);
        assert!(
            !out.status.success(),
            "{case}: expected configure to fail on a retired ENTITIES argument"
        );
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(
            err.contains("ENTITIES was retired"),
            "{case}: expected the ENTITIES tombstone, got:\n{err}"
        );
        // The refusal has to name where the facts live now, or it is a remedy
        // the reader cannot follow — which is exactly what three `message()`
        // strings elsewhere in cmake had become, still naming this very
        // argument 24 days after it started raising FATAL_ERROR.
        assert!(
            err.contains("contract.yaml") && err.contains("system.toml"),
            "{case}: the tombstone must name BOTH live surfaces (bringup \
             sidecar, standalone leaf), got:\n{err}"
        );
    }
}

/// Negative control for the test above. A call with no `ENTITIES` must not
/// reach the tombstone — asserted on the DIAGNOSTIC, not on success, because
/// this harness stages no board and the configure may fail for its own
/// unrelated reasons. A refusal that fires on every call would satisfy the
/// positive cases just as well.
#[test]
fn a_call_without_entities_never_reaches_the_tombstone() {
    if !nros_tests::process::require_cmake() {
        nros_tests::skip!("cmake not on PATH");
    }
    let body = "nano_ros_node_register(\n  NAME talker\n  CLASS demo::Talker\n  SOURCES src/dummy.cpp\n  DEPLOY native)\n";
    let (_g, root, build) = stage(body, "talker_pkg");
    let out = configure(&root, &build);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        !err.contains("ENTITIES was retired"),
        "a call with no ENTITIES argument raised the tombstone:\n{err}"
    );
}
