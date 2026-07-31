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
fn codegen_edition_std_msgs_in_container() {
    let ed = ros_env::test_edition();
    let env = DockerRosEnv::new(&ed, Middleware::Cyclonedds { domain_id: 1 });
    if !env.available() {
        nros_tests::skip!(
            "{ed} image not built or docker absent — run `just ros_editions image {ed}`"
        );
    }
    let Some(nros_bin) = ros_env::host_nros_bin() else {
        nros_tests::skip!("host `nros` binary not found — run `just setup-cli`");
    };

    let out = tempfile::tempdir().expect("tempdir");
    let gen_dir = out.path().join(&ed);

    // Generate from std_msgs' installed manifest. `nros generate-rust` resolves
    // + emits the packages reachable from that manifest (here builtin_interfaces,
    // std_msgs' dependency) against the EDITION's definitions. (Selecting an exact
    // target package uses the `nros sync` workspace flow — a W5 concern; this
    // W4 test proves only that codegen RUNS per-edition, in-container, producing
    // real bindings from the edition's own defs.)
    let result = env
        .generate(
            &nros_bin,
            &format!("/opt/ros/{ed}/share/std_msgs"),
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

/// The committed per-edition golden of geometry_msgs' generated message set, and
/// a generated manifest, must match — so an edition-def change (add/remove a
/// message) is caught. The goldens genuinely differ per edition (jazzy=33 vs
/// iron=30 modules — jazzy adds polygon_instance*, velocity_with_covariance_*),
/// so this also proves the codegen is edition-discriminating, not a no-op.
#[test]
fn codegen_geometry_msgs_matches_edition_golden() {
    use std::path::PathBuf;

    let ed = ros_env::test_edition();
    let env = DockerRosEnv::new(&ed, Middleware::Cyclonedds { domain_id: 1 });
    if !env.available() {
        nros_tests::skip!(
            "{ed} image not built or docker absent — run `just ros_editions image {ed}`"
        );
    }
    let Some(nros_bin) = ros_env::host_nros_bin() else {
        nros_tests::skip!("host `nros` binary not found — run `just setup-cli`");
    };

    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/ros-editions");
    let golden_path = fixtures.join(&ed).join("geometry_msgs-modules.txt");
    let Ok(golden_body) = std::fs::read_to_string(&golden_path) else {
        nros_tests::skip!("no geometry_msgs golden for edition {ed} at {golden_path:?}");
    };
    // Sort the golden the same way the manifest is sorted (Rust byte order), so
    // the compare is order-independent of how the golden file was written.
    let mut golden: Vec<String> = golden_body
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect();
    golden.sort();

    // Copy the committed consumer manifest to a scratch dir (the container writes
    // a codegen sig into the workdir; keep the fixture pristine).
    let scratch = tempfile::tempdir().expect("tempdir");
    let consumer = scratch.path().join("consumer");
    std::fs::create_dir_all(&consumer).unwrap();
    std::fs::copy(
        fixtures.join("geometry-consumer/package.xml"),
        consumer.join("package.xml"),
    )
    .expect("copy consumer package.xml");
    let out = scratch.path().join("out");

    let result = env
        .generate_from_consumer(&nros_bin, &consumer, &out)
        .expect("in-container generate");
    assert!(
        result.status.success(),
        "generate_from_consumer failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );

    let manifest = geometry_msg_modules(&out.join("geometry_msgs/src/msg"));
    assert!(
        !manifest.is_empty(),
        "no geometry_msgs message modules generated in {out:?}"
    );
    assert_eq!(
        manifest, golden,
        "geometry_msgs generated message set for {ed} drifted from the golden \
         {golden_path:?} (a ROS {ed} message def changed, or codegen dropped/added \
         a message). Re-seed the golden if this is an intended def change."
    );
}

/// Sorted `*.rs` message-module basenames under a generated `.../src/msg` dir
/// (excluding `mod.rs`) — the stable "message set" fingerprint.
fn geometry_msg_modules(msg_dir: &std::path::Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(msg_dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            (name.ends_with(".rs") && name != "mod.rs").then_some(name)
        })
        .collect();
    names.sort();
    names
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
