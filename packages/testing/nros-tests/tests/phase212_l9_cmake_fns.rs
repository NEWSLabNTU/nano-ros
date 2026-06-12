//! Phase 212.L.9 — C++ cmake fn surface tests for the three
//! Phase 212.L pkg shapes.
//!
//! Coverage:
//!
//! 1. `nano_ros_node_register_emits_metadata` — configure-time
//!    invocation writes `${CMAKE_BINARY_DIR}/nros-metadata.json` with
//!    the expected component entry shape (name / class / sources /
//!    deploy / pkg_dir / lang).
//! 2. `nano_ros_node_register_rejects_class_pkg_mismatch` —
//!    CLASS without `${PROJECT_NAME}::` prefix → cmake FATAL_ERROR
//!    (L.4 rule).
//! 3. `nano_ros_application_rejects_embedded_deploy` —
//!    `nano_ros_application(... DEPLOY zephyr)` → cmake FATAL_ERROR
//!    (L.2 rule: Application pkgs are native-only).
//! 4. `nano_ros_deploy_records_target_config` —
//!    `nano_ros_deploy(TARGET native RMW zenoh DOMAIN_ID 7)` →
//!    metadata `deploy_targets.native` carries the recorded fields.
//!
//! Skip semantics mirror `phase212_d_workspace_metadata.rs`: cleanly
//! `nros_tests::skip!` when cmake isn't on PATH.

use std::{fs, path::PathBuf, process::Command};

fn workspace_root() -> PathBuf {
    nros_tests::project_root()
}

fn cmake_module_path() -> PathBuf {
    workspace_root().join("cmake/NanoRosNodeRegister.cmake")
}

fn require_prereqs() -> bool {
    nros_tests::process::require_cmake()
}

/// Stage a fresh fixture dir + write CMakeLists.txt + dummy src.
/// Returns (tempdir guard, root, build_dir).
fn stage(cmake_body: &str, project_name: &str) -> (tempfile::TempDir, PathBuf, PathBuf) {
    let guard = tempfile::tempdir().expect("tempdir");
    let root = guard.path().to_path_buf();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("src/dummy.cpp"),
        "int phase212_l9_stub() { return 0; }\n",
    )
    .unwrap();
    fs::write(
        root.join("src/dummy.c"),
        "#include <nros/node_pkg.h>\n\
         static nros_ret_t register_talker(nros_node_context_t* ctx) { (void)ctx; return NROS_RET_OK; }\n\
         NROS_NODE_REGISTER(register_talker);\n",
    )
    .unwrap();
    let cml = format!(
        "cmake_minimum_required(VERSION 3.22)\n\
         project({project_name} C CXX)\n\
         include(\"{module}\")\n\
         {body}\n",
        module = cmake_module_path().display(),
        body = cmake_body,
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
fn nano_ros_node_register_emits_metadata() {
    if !require_prereqs() {
        nros_tests::skip!("cmake not on PATH");
    }
    let body = "nano_ros_node_register(\n  \
                NAME talker\n  CLASS talker_pkg::Talker\n  \
                SOURCES src/dummy.cpp\n  DEPLOY native zephyr)\n";
    let (_g, root, build) = stage(body, "talker_pkg");
    let out = configure(&root, &build);
    assert!(
        out.status.success(),
        "cmake configure failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let meta = build.join("nros-metadata.json");
    assert!(meta.is_file(), "missing {}", meta.display());
    let body = fs::read_to_string(&meta).expect("read metadata");
    assert!(
        body.contains("\"name\": \"talker\"") && body.contains("\"class\": \"talker_pkg::Talker\""),
        "metadata missing component entry:\n{body}"
    );
    assert!(
        body.contains("\"sources\": [\"src/dummy.cpp\"]"),
        "metadata sources mismatch:\n{body}"
    );
    assert!(
        body.contains("\"deploy\": [\"native\", \"zephyr\"]"),
        "metadata deploy mismatch:\n{body}"
    );
    assert!(
        body.contains("\"lang\": \"cpp\""),
        "metadata lang mismatch:\n{body}"
    );
    // Node STATIC lib must be addressable.
    let body_lc = body.to_lowercase();
    assert!(
        body_lc.contains("\"pkg_dir\""),
        "metadata missing pkg_dir field:\n{body}"
    );
}

#[test]
fn nano_ros_node_register_accepts_c_language() {
    if !require_prereqs() {
        nros_tests::skip!("cmake not on PATH");
    }
    let body = "nano_ros_node_register(\n  \
                NAME talker\n  CLASS c_talker_pkg::Talker\n  \
                LANGUAGE C\n  SOURCES src/dummy.c\n  DEPLOY native)\n";
    let (_g, root, build) = stage(body, "c_talker_pkg");
    let out = configure(&root, &build);
    assert!(
        out.status.success(),
        "cmake configure failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let meta = build.join("nros-metadata.json");
    let body = fs::read_to_string(&meta).expect("read metadata");
    assert!(
        body.contains("\"class\": \"c_talker_pkg::Talker\""),
        "metadata class mismatch:\n{body}"
    );
    assert!(
        body.contains("\"sources\": [\"src/dummy.c\"]"),
        "metadata sources mismatch:\n{body}"
    );
    assert!(
        body.contains("\"lang\": \"c\""),
        "metadata lang mismatch:\n{body}"
    );
}

#[test]
fn nano_ros_node_register_rejects_class_pkg_mismatch() {
    if !require_prereqs() {
        nros_tests::skip!("cmake not on PATH");
    }
    let body = "nano_ros_node_register(\n  \
                NAME talker\n  CLASS wrong_pkg::Talker\n  \
                SOURCES src/dummy.cpp\n  DEPLOY native)\n";
    let (_g, root, build) = stage(body, "talker_pkg");
    let out = configure(&root, &build);
    assert!(
        !out.status.success(),
        "expected cmake configure to fail on CLASS pkg mismatch"
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("must start with 'talker_pkg::'") || err.contains("Phase 212.L.4"),
        "expected L.4 diagnostic, got:\n{err}"
    );
}

#[test]
fn nano_ros_application_rejects_embedded_deploy() {
    if !require_prereqs() {
        nros_tests::skip!("cmake not on PATH");
    }
    let body = "nano_ros_application(\n  \
                NAME my_app\n  SOURCES src/dummy.cpp\n  \
                DEPLOY native zephyr)\n";
    let (_g, root, build) = stage(body, "my_app");
    let out = configure(&root, &build);
    assert!(
        !out.status.success(),
        "expected cmake configure to fail on embedded DEPLOY in Application"
    );
    let err = String::from_utf8_lossy(&out.stderr);
    // Issue #34 — `nano_ros_application` is now a deprecated shim that forwards
    // to `nano_ros_entry` (Phase 212.N.7 rename), so an embedded `DEPLOY` is
    // still rejected but with the entry-layer's board-centric diagnostic rather
    // than the old L.2 "native-only" wording. Accept either so this drift-guard
    // tracks the current message without losing the behavioural check (embedded
    // deploy must be rejected, asserted above).
    assert!(
        err.contains("native-only")
            || err.contains("Phase 212.L.2")
            || err.contains("embedded Entry pkgs need a Board")
            || err.contains("rejected"),
        "expected an embedded-deploy rejection diagnostic, got:\n{err}"
    );
}

#[test]
fn nano_ros_deploy_records_target_config() {
    if !require_prereqs() {
        nros_tests::skip!("cmake not on PATH");
    }
    let body = "nano_ros_deploy(TARGET native RMW zenoh DOMAIN_ID 7)\n\
                nano_ros_deploy(TARGET zephyr RMW cyclonedds DOMAIN_ID 3 \
                LOCATOR tcp/10.0.0.1:7447)\n";
    let (_g, root, build) = stage(body, "demo_pkg");
    let out = configure(&root, &build);
    assert!(
        out.status.success(),
        "cmake configure failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let meta = build.join("nros-metadata.json");
    let body = fs::read_to_string(&meta).expect("read metadata");
    assert!(
        body.contains("\"native\": {\"rmw\": \"zenoh\", \"domain_id\": 7, \"locator\": null}"),
        "missing native deploy_targets entry:\n{body}"
    );
    assert!(
        body.contains(
            "\"zephyr\": {\"rmw\": \"cyclonedds\", \"domain_id\": 3, \
             \"locator\": \"tcp/10.0.0.1:7447\"}"
        ),
        "missing zephyr deploy_targets entry:\n{body}"
    );
}
