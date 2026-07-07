//! Phase 212.K.4 — `nros codegen cyclonedds-descriptors` integration.
//!
//! Drives the installed `nros` CLI against a tempdir-staged
//! `<pkg>/msg/<Name>.msg` and verifies the verb emits:
//!
//!   1. the `<pkg>_<Msg>.{c,h}` pair via the host `idlc`
//!   2. a `register.{c,h}` translation unit w/ a single
//!      `extern "C" void <crate>_register_descriptors(void)` entry
//!   3. a `cyclonedds-descriptors.json` manifest listing every entry
//!
//! Two coverage points map 1:1 to the verb's observable contract:
//!
//! * `codegen_cyclonedds_emits_std_msgs`
//! * `nros_codegen_cyclonedds_descriptors_emits_register_tu`
//!
//! Both skip cleanly via `nros_tests::skip!` when the prerequisites
//! (`nros` CLI + a host `idlc`) aren't present.

use std::{fs, path::PathBuf, process::Command};

/// Locate the Phase 212.K.1 host `idlc` that lives under the project's
/// `build/cyclonedds/bin/idlc`. The descriptor verb requires a real
/// `idlc` — no stub fallback (the existing K.2 build.rs already gates
/// on the same path, so any setup that runs K.2 also has it).
fn idlc_path() -> Option<PathBuf> {
    // Legacy host-install location (pre-186.6 `just cyclonedds setup`).
    let candidate = nros_tests::project_root().join("build/cyclonedds/bin/idlc");
    if candidate.is_file() {
        return Some(candidate);
    }
    // Phase 186.6 dropped the build/cyclonedds install — idlc resolves from
    // PATH (a ROS 2 install or the source build), same as the backend's
    // NrosRmwCycloneddsTypeSupport.cmake `find_program(IDLC_EXECUTABLE idlc)`.
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join("idlc"))
        .find(|p| p.is_file())
}

/// Skip-or-proceed: every K.4 test needs both the `nros` CLI and `idlc`.
fn require_preconditions() -> Option<(PathBuf, PathBuf)> {
    if !nros_tests::require_nros_cli() {
        return None;
    }
    let Some(idlc) = idlc_path() else {
        eprintln!(
            "Skipping test: idlc not found at build/cyclonedds/bin/idlc \
             (run `just cyclonedds setup`)"
        );
        return None;
    };
    let nros = nros_tests::nros_cli_bin_path().expect("nros CLI resolved");
    Some((nros, idlc))
}

/// Stage `<root>/std_msgs/msg/Int32.msg` + `<root>/include` and return
/// the message-source path the verb should pick up.
fn stage_std_msgs_int32(root: &std::path::Path) -> PathBuf {
    let msg_dir = root.join("std_msgs/msg");
    fs::create_dir_all(&msg_dir).expect("mkdir std_msgs/msg");
    let msg_path = msg_dir.join("Int32.msg");
    fs::write(&msg_path, "int32 data\n").expect("write Int32.msg");
    fs::create_dir_all(root.join("include")).expect("mkdir include");
    msg_path
}

/// Verifies CycloneDDS descriptor codegen emits C for `std_msgs/Int32`.
#[test]
fn codegen_cyclonedds_emits_std_msgs() {
    let Some((nros, idlc)) = require_preconditions() else {
        nros_tests::skip!("nros CLI or idlc not available");
    };

    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    let msg_path = stage_std_msgs_int32(root);
    let include = root.join("include");
    let out = root.join("out");

    let status = Command::new(&nros)
        .args(["codegen", "cyclonedds-descriptors"])
        .arg("--idlc")
        .arg(&idlc)
        .arg("--include")
        .arg(&include)
        .arg("--msg")
        .arg(format!("std_msgs/Int32={}", msg_path.display()))
        .arg("--crate-name")
        .arg("phase212_k4_int32_example")
        .arg("--out")
        .arg(&out)
        .status()
        .expect("spawn nros");
    assert!(status.success(), "nros codegen failed: {status}");

    assert!(
        out.join("std_msgs_Int32.idl").is_file(),
        "missing std_msgs_Int32.idl"
    );
    assert!(
        out.join("std_msgs_Int32.c").is_file(),
        "missing std_msgs_Int32.c"
    );
    assert!(
        out.join("std_msgs_Int32.h").is_file(),
        "missing std_msgs_Int32.h"
    );

    let manifest_path = out.join("cyclonedds-descriptors.json");
    let body = fs::read_to_string(&manifest_path).expect("read manifest");
    let doc: serde_json::Value = serde_json::from_str(&body).expect("parse manifest");

    assert_eq!(doc["crate_name"], "phase212_k4_int32_example");
    let descs = doc["descriptors"].as_array().expect("descriptors array");
    assert_eq!(descs.len(), 1, "one descriptor expected");
    assert_eq!(descs[0]["pkg"], "std_msgs");
    assert_eq!(descs[0]["msg"], "Int32");
    assert_eq!(descs[0]["type_name"], "std_msgs::msg::dds_::Int32_");
    assert_eq!(
        descs[0]["descriptor_symbol"],
        "std_msgs_msg_dds__Int32__desc"
    );
}

#[test]
fn nros_codegen_cyclonedds_descriptors_emits_register_tu() {
    let Some((nros, idlc)) = require_preconditions() else {
        nros_tests::skip!("nros CLI or idlc not available");
    };

    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    let msg_path = stage_std_msgs_int32(root);
    let include = root.join("include");
    let out = root.join("out");

    let status = Command::new(&nros)
        .args(["codegen", "cyclonedds-descriptors"])
        .arg("--idlc")
        .arg(&idlc)
        .arg("--include")
        .arg(&include)
        .arg("--msg")
        .arg(format!("std_msgs/Int32={}", msg_path.display()))
        .arg("--crate-name")
        .arg("phase212_k4_register_tu")
        .arg("--out")
        .arg(&out)
        .status()
        .expect("spawn nros");
    assert!(status.success(), "nros codegen failed: {status}");

    let reg_h = fs::read_to_string(out.join("register.h")).expect("read register.h");
    assert!(
        reg_h.contains("void phase212_k4_register_tu_register_descriptors(void);"),
        "register.h missing entry decl: {reg_h}"
    );

    let reg_c = fs::read_to_string(out.join("register.c")).expect("read register.c");
    assert!(
        reg_c.contains("#include \"std_msgs_Int32.h\""),
        "register.c missing generated include: {reg_c}"
    );
    assert!(
        reg_c.contains("void phase212_k4_register_tu_register_descriptors(void) {"),
        "register.c missing entry body: {reg_c}"
    );
    assert!(
        reg_c.contains("nros_rmw_cyclonedds_register_descriptor("),
        "register.c missing register call: {reg_c}"
    );
    assert!(
        reg_c.contains("\"std_msgs::msg::dds_::Int32_\""),
        "register.c missing type-name literal: {reg_c}"
    );
    assert!(
        reg_c.contains("&std_msgs_msg_dds__Int32__desc"),
        "register.c missing descriptor symbol: {reg_c}"
    );
    assert!(
        reg_c.contains("__attribute__((constructor))"),
        "register.c missing whole-archive-friendly constructor hook: {reg_c}"
    );
}
