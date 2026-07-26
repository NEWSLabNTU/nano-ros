//! phase-309 W4 — codegen axis: generate nano-ros bindings against an EDITION's
//! own message definitions, inside that edition's container.
//!
//! This proves `nros generate-rust` runs against a ROS 2 edition the host does
//! not have installed (the host-built `nros` binary is bind-mounted into the
//! edition image; glibc is backward-compatible). It complements the offline +
//! live type-hash oracle (`rosidl-bindgen/tests/edition_{hash_oracle,
//! type_hash_offline}.rs`, phase-304), which already pins the wire-critical
//! RIHS01 hashes engine == fixture == live Jazzy.
//!
//! Skips (never silently passes) when docker, the image, or the host `nros`
//! binary is absent. Not part of `just ci`; the `ros_editions ci` composite
//! (W6) runs it.

use nros_tests::ros_env::{self, DockerRosEnv, Middleware, RosEnv};

#[test]
fn codegen_jazzy_std_msgs_in_container() {
    let env = DockerRosEnv::new("jazzy", Middleware::Cyclonedds { domain_id: 1 });
    if !env.available() {
        nros_tests::skip!(
            "jazzy image not built or docker absent — run `just ros_editions image jazzy`"
        );
    }
    let Some(nros_bin) = ros_env::host_nros_bin() else {
        nros_tests::skip!("host `nros` binary not found — run `just setup-cli`");
    };

    let out = tempfile::tempdir().expect("tempdir");
    let gen_dir = out.path().join("jazzy");

    // Generate from std_msgs' installed manifest. `nros generate-rust` resolves
    // + emits the packages reachable from that manifest (here builtin_interfaces,
    // std_msgs' dependency) against the JAZZY definitions. (Selecting an exact
    // target package uses the `nros ws sync` workspace flow — a W5 concern; this
    // W4 test proves only that codegen RUNS per-edition, in-container, producing
    // real bindings from the edition's own defs.)
    let result = env
        .generate(
            &nros_bin,
            "/opt/ros/jazzy/share/std_msgs",
            &gen_dir,
            &["--force"],
        )
        .expect("run in-container codegen");

    let log = format!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        result.status.success(),
        "in-container `nros generate-rust` failed:\n{log}"
    );

    // builtin_interfaces (Time/Duration) is generated against the jazzy defs and
    // must carry real Rust source (not empty stubs), and land on the host via the
    // bind-mount — proving the whole in-container codegen path end-to-end.
    let pkg = gen_dir.join("builtin_interfaces");
    assert!(
        pkg.is_dir(),
        "expected builtin_interfaces bindings in {gen_dir:?}\n{log}"
    );
    assert!(
        walk_has_rs(&pkg),
        "builtin_interfaces bindings contain no .rs source in {pkg:?}\n{log}"
    );
}

/// Any `.rs` file under `dir` (recursively) that is non-empty?
fn walk_has_rs(dir: &std::path::Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            if walk_has_rs(&p) {
                return true;
            }
        } else if p.extension().is_some_and(|x| x == "rs")
            && std::fs::metadata(&p).map(|m| m.len() > 0).unwrap_or(false)
        {
            return true;
        }
    }
    false
}
