//! Integration test for the codegen version guard between the `nros` binary
//! and the nano-ros runtime a consumer links.
//!
//! Phase 218.E wrote this against a synthetic `Cargo.lock` pinning `nros-core`
//! at a wrong SemVer. Phase-429 W2 re-tokened the guard, so the fixture is now
//! a synthetic nano-ros RUNTIME TREE — a `packages/core/nros-core/Cargo.toml`
//! marker plus a `codegen_version.rs` declaring an accepted range — and there
//! is deliberately **no `Cargo.lock` anywhere in it**. That absence is the
//! point: a C or C++ consumer has no lock, and under the old token the guard
//! was silently skipped for exactly those users. `no_cargo_lock_anywhere`
//! asserts it stays absent, so a future refactor cannot quietly reintroduce a
//! lock dependency and keep these tests green.
//!
//! Both directions are observed, on the two verbs that matter:
//!
//! * a runtime that REFUSES this binary's emission → non-zero exit naming both
//!   numbers, on `generate-rust` (an old call site) and on `build` (a new one);
//! * a runtime that ACCEPTS it → the guard is silent and the verb proceeds;
//! * `NROS_SKIP_VERSION_CHECK=1` → bypassed, with the `warning:` line visible.
//!
//! Plus the two read-only doors: `nros --codegen-version` and `nros version`.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use nros_cli_core::abi_guard::EMITTED_VERSION;

fn temp_root(tag: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir =
        std::env::temp_dir().join(format!("phase-429-w2-{tag}-{}-{stamp}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A synthetic nano-ros runtime tree accepting `min..=max`, with a minimal
/// `package.xml` at its root so codegen verbs have a manifest to point at.
///
/// No `Cargo.lock`: this is the C/C++ consumer shape.
fn write_runtime_fixture(root: &Path, min: u32, max: u32) {
    fs::create_dir_all(root.join("packages/core/nros-core/src")).unwrap();
    // The marker `find_monorepo_root` walks up for.
    fs::write(
        root.join("packages/core/nros-core/Cargo.toml"),
        "# synthetic runtime-tree marker\n",
    )
    .unwrap();
    fs::write(
        root.join("packages/core/nros-core/src/codegen_version.rs"),
        format!(
            "//! Synthetic. Prose mentions NROS_CODEGEN_VERSION first on purpose.\n\
             pub const NROS_CODEGEN_VERSION: u32 = {max};\n\
             pub const NROS_CODEGEN_VERSION_MIN: u32 = {min};\n"
        ),
    )
    .unwrap();

    fs::write(
        root.join("package.xml"),
        r#"<?xml version="1.0"?>
<package format="3">
  <name>abi_guard_fixture</name>
  <version>0.0.1</version>
  <description>phase-429 W2 codegen version guard test fixture.</description>
  <maintainer email="test@example.com">test</maintainer>
  <license>Apache-2.0</license>
  <buildtool_depend>ament_cargo</buildtool_depend>
  <export>
    <build_type>ament_cargo</build_type>
  </export>
</package>
"#,
    )
    .unwrap();
}

/// The fixture must never grow a `Cargo.lock` — see the module docs.
fn no_cargo_lock_anywhere(root: &Path) {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else {
                assert_ne!(
                    path.file_name().and_then(|n| n.to_str()),
                    Some("Cargo.lock"),
                    "fixture grew a Cargo.lock at {} — the guard must not need one",
                    path.display(),
                );
            }
        }
    }
}

/// Path to the `nros` binary — cargo sets `CARGO_BIN_EXE_nros` because
/// `nros-cli` declares `[[bin]] name = "nros"`.
fn nros_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_nros"))
}

/// A range that cannot contain `EMITTED_VERSION`, whichever way it moves.
fn rejecting_range() -> (u32, u32) {
    (EMITTED_VERSION + 5, EMITTED_VERSION + 9)
}

fn run_nros(args: &[&std::ffi::OsStr], skip: Option<&str>) -> (bool, String) {
    let mut cmd = Command::new(nros_bin());
    cmd.args(args);
    match skip {
        Some(v) => cmd.env("NROS_SKIP_VERSION_CHECK", v),
        None => cmd.env_remove("NROS_SKIP_VERSION_CHECK"),
    };
    let out = cmd.output().expect("spawn nros");
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), combined)
}

#[test]
fn a_rejecting_runtime_refuses_generate_rust_with_no_cargo_lock_in_sight() {
    let root = temp_root("generate_refused");
    let (min, max) = rejecting_range();
    write_runtime_fixture(&root, min, max);
    no_cargo_lock_anywhere(&root);

    let (ok, out) = run_nros(
        &[
            "generate-rust".as_ref(),
            "--manifest".as_ref(),
            root.join("package.xml").as_os_str(),
            "--output".as_ref(),
            root.join("generated").as_os_str(),
        ],
        None,
    );

    assert!(!ok, "expected non-zero exit; got success.\n{out}");
    assert!(out.contains("ABI version mismatch"), "{out}");
    assert!(out.contains("nros generate-rust"), "{out}");
    assert!(
        out.contains(&format!("CLI emits codegen version:    {EMITTED_VERSION}")),
        "expected the emitted version {EMITTED_VERSION} in the message.\n{out}",
    );
    assert!(
        out.contains(&format!("{min}..={max}")),
        "expected the runtime's accepted range {min}..={max}.\n{out}",
    );
    // Refused BEFORE emitting: nothing was written to --output.
    assert!(
        !root.join("generated").exists(),
        "the guard must fire before anything is emitted",
    );
    no_cargo_lock_anywhere(&root);

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn an_accepting_runtime_lets_generate_rust_proceed() {
    let root = temp_root("generate_accepted");
    // A range that contains this binary's emission.
    write_runtime_fixture(&root, 0, EMITTED_VERSION + 1);

    let (_ok, out) = run_nros(
        &[
            "generate-rust".as_ref(),
            "--manifest".as_ref(),
            root.join("package.xml").as_os_str(),
            "--output".as_ref(),
            root.join("generated").as_os_str(),
        ],
        None,
    );

    // Exit code is NOT pinned: the synthetic package.xml may legitimately fail
    // downstream codegen. What must hold is that the guard did not fire.
    assert!(
        !out.contains("ABI version mismatch"),
        "guard fired against a runtime that accepts this binary.\n{out}",
    );

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn nros_build_is_guarded_too() {
    // `nros build` is RFC-0065's front door and had no guard at all before
    // phase-429 W2. The refusal must beat planning, which is the stage that
    // writes the generated root.
    let root = temp_root("build_refused");
    let (min, max) = rejecting_range();
    write_runtime_fixture(&root, min, max);

    let (ok, out) = run_nros(
        &["build".as_ref(), "--workspace".as_ref(), root.as_os_str()],
        None,
    );

    assert!(!ok, "expected non-zero exit; got success.\n{out}");
    assert!(out.contains("ABI version mismatch"), "{out}");
    assert!(out.contains("nros build"), "{out}");
    // The guard beat planning: no build tree was generated.
    assert!(!root.join("build").exists(), "guard must precede planning");

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn nros_sync_is_guarded_too() {
    let root = temp_root("sync_refused");
    let (min, max) = rejecting_range();
    write_runtime_fixture(&root, min, max);

    let (ok, out) = run_nros(&["sync".as_ref(), root.as_os_str()], None);

    assert!(!ok, "expected non-zero exit; got success.\n{out}");
    assert!(out.contains("ABI version mismatch"), "{out}");
    assert!(out.contains("nros sync"), "{out}");
    assert!(
        !root.join("generated").exists(),
        "guard must precede the first write",
    );

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn the_env_opt_out_bypasses_the_guard_and_says_so() {
    let root = temp_root("opt_out");
    let (min, max) = rejecting_range();
    write_runtime_fixture(&root, min, max);

    let (_ok, out) = run_nros(
        &[
            "generate-rust".as_ref(),
            "--manifest".as_ref(),
            root.join("package.xml").as_os_str(),
            "--output".as_ref(),
            root.join("generated").as_os_str(),
        ],
        Some("1"),
    );

    assert!(
        !out.contains("ABI version mismatch"),
        "guard fired despite NROS_SKIP_VERSION_CHECK=1.\n{out}",
    );
    assert!(
        out.contains("ABI version guard bypassed"),
        "the bypass must be visible in CI logs.\n{out}",
    );

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn the_binary_reports_the_version_it_emits() {
    let (ok, out) = run_nros(&["--codegen-version".as_ref()], None);
    assert!(ok, "`nros --codegen-version` must succeed.\n{out}");
    assert_eq!(
        out.trim(),
        EMITTED_VERSION.to_string(),
        "`--codegen-version` prints the bare number and nothing else",
    );

    let (ok, out) = run_nros(&["version".as_ref()], None);
    assert!(ok, "`nros version` must succeed.\n{out}");
    assert!(
        out.contains(&format!("codegen version {EMITTED_VERSION}")),
        "`nros version` must name the codegen version.\n{out}",
    );
}
