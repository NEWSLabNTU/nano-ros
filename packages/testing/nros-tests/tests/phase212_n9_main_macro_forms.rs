//! Phase 212.N.9 — `nros::main!()` proc-macro round-trip.
//!
//! Stages the `n9_workspace` fixture into a tempdir, rewrites the
//! `@NANO_ROS_ROOT@` placeholders to absolute `path =` deps (same
//! pattern as `phase212_macro_one_dep.rs`), and runs `cargo check`
//! against each of the four `nros::main!()` forms in turn.
//!
//! ## Forms exercised
//!
//! 1. `nros::main!();`
//!    → reads `[package.metadata.nros.entry] deploy = "native"`,
//!      emits `::demo_entry::register(runtime)?;` (lib-self bringup).
//! 2. `nros::main!(board = ::nros_board_native::NativeBoard);`
//!    → same as #1, with explicit board.
//! 3. `nros::main!(launch = "demo_bringup");`
//!    → consults the workspace pkg-index (N.10) for the bringup
//!      pkg, reads `system.toml::default_launch` → walks the launch
//!      XML (N.11) → emits `::talker_pkg::register(runtime)?;`.
//! 4. `nros::main!(board = …, launch = "demo_bringup:sim.launch.xml",
//!    args = [("use_sim", "true")]);`
//!    → everything explicit; alternate launch file picked up.
//!
//! ## Rebuild-tracking workaround
//!
//! The macro emits `const _: &[u8] = include_bytes!("/abs/path");`
//! for every file it read at expansion time. Stable Rust can't use
//! `proc_macro::tracked_path::path()`; this test confirms a
//! follow-up `cargo check` after touching the launch.xml re-checks
//! the Entry pkg.
//!
//! Run with: `cargo test -p nros-tests --test phase212_n9_main_macro_forms`

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn workspace_root() -> PathBuf {
    nros_tests::project_root()
}

fn fixture_src() -> PathBuf {
    workspace_root().join("packages/testing/nros-tests/fixtures/n9_workspace")
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
    for entry in walk(root)? {
        if !entry.is_file() {
            continue;
        }
        let Ok(text) = fs::read_to_string(&entry) else {
            continue;
        };
        if !text.contains("@NANO_ROS_ROOT@") {
            continue;
        }
        fs::write(&entry, text.replace("@NANO_ROS_ROOT@", replacement))?;
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

/// Overwrite the staged Entry pkg's `main.rs` with the supplied body
/// (a `nros::main!(...);` line) and run `cargo check`. Returns the
/// raw stdout / stderr for assertion.
fn check_main_variant(root: &Path, label: &str, body: &str) {
    let main_rs = root.join("src/demo_entry/src/main.rs");
    let header = format!("//! Phase 212.N.9 fixture — {label}.\n\n");
    fs::write(&main_rs, format!("{header}{body}\n")).expect("write demo_entry/src/main.rs");
    let out = Command::new("cargo")
        .args(["check", "-p", "demo_entry", "--manifest-path"])
        .arg(root.join("Cargo.toml"))
        .output()
        .expect("spawn cargo check");
    assert!(
        out.status.success(),
        "`cargo check` failed for form `{label}`.\n\
        body:\n{body}\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}

#[test]
fn n9_main_macro_form1_no_args_compiles() {
    let (_g, root) = stage_fixture();
    // Form 1 — reads `[package.metadata.nros.entry] deploy = "native"`
    // and emits `::demo_entry::register(runtime)?;` (the bin target's
    // sibling `lib.rs`). The `nros::node!(DemoEntry)` macro in the
    // fixture's `lib.rs` defines `register`.
    check_main_variant(&root, "form 1 (no args)", "nros::main!();");
}

#[test]
fn n9_main_macro_form2_board_only_compiles() {
    let (_g, root) = stage_fixture();
    check_main_variant(
        &root,
        "form 2 (board only)",
        "nros::main!(board = ::nros_board_native::NativeBoard);",
    );
}

#[test]
fn n9_main_macro_form3_launch_default_compiles() {
    let (_g, root) = stage_fixture();
    // Form 3 — `launch = "demo_bringup"` triggers N.10 pkg-index walk
    // + N.11 launch parse against `demo_bringup/launch/system.launch.xml`
    // (per `system.toml::default_launch`). The emitted body
    // dispatches `::talker_pkg::register(runtime)?;` for the single
    // `<node pkg="talker_pkg" exec="talker"/>` entry.
    check_main_variant(
        &root,
        "form 3 (launch, default file)",
        "nros::main!(launch = \"demo_bringup\");",
    );
}

#[test]
fn n9_main_macro_form4_all_explicit_compiles() {
    let (_g, root) = stage_fixture();
    check_main_variant(
        &root,
        "form 4 (all explicit)",
        "nros::main!(\n    \
            board = ::nros_board_native::NativeBoard,\n    \
            launch = \"demo_bringup:sim.launch.xml\",\n    \
            args = [(\"use_sim\", \"true\")],\n\
        );",
    );
}

#[test]
fn n9_main_macro_unknown_board_emits_compile_error() {
    let (_g, root) = stage_fixture();
    // Override Cargo.toml's `deploy = "native"` to something the
    // macro's board lookup table doesn't recognise.
    let cargo_toml = root.join("src/demo_entry/Cargo.toml");
    let raw = fs::read_to_string(&cargo_toml).expect("read demo_entry Cargo.toml");
    let bad = raw.replace("deploy = \"native\"", "deploy = \"frobnicator\"");
    assert_ne!(raw, bad, "expected `deploy = \"native\"` line in fixture");
    fs::write(&cargo_toml, bad).expect("write bad Cargo.toml");

    fs::write(root.join("src/demo_entry/src/main.rs"), "nros::main!();\n").expect("write main.rs");
    let out = Command::new("cargo")
        .args(["check", "-p", "demo_entry", "--manifest-path"])
        .arg(root.join("Cargo.toml"))
        .output()
        .expect("spawn cargo check");
    assert!(
        !out.status.success(),
        "expected `cargo check` to fail on unknown board `frobnicator`, but it succeeded.\n\
        stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unknown board"),
        "expected diagnostic mentioning `unknown board`, stderr:\n{stderr}",
    );
}

#[test]
fn n9_main_macro_rebuilds_on_launch_xml_touch() {
    let (_g, root) = stage_fixture();
    fs::write(
        root.join("src/demo_entry/src/main.rs"),
        "nros::main!(launch = \"demo_bringup\");\n",
    )
    .expect("write main.rs");
    let out = Command::new("cargo")
        .args(["check", "-p", "demo_entry", "--manifest-path"])
        .arg(root.join("Cargo.toml"))
        .output()
        .expect("first cargo check");
    assert!(
        out.status.success(),
        "initial cargo check failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );

    // Touch the launch.xml — the macro's `include_bytes!` stamp
    // should force a re-check. Sleep just long enough that the new
    // mtime exceeds cargo's fingerprint resolution; then rewrite the
    // file (writing identical content still bumps mtime).
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let launch_xml = root.join("src/demo_bringup/launch/system.launch.xml");
    let body = fs::read_to_string(&launch_xml).expect("read launch.xml");
    fs::write(&launch_xml, &body).expect("rewrite launch.xml");

    let out2 = Command::new("cargo")
        .args(["check", "-p", "demo_entry", "--manifest-path"])
        .arg(root.join("Cargo.toml"))
        .output()
        .expect("second cargo check");
    assert!(
        out2.status.success(),
        "second cargo check (post-touch) failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out2.stdout),
        String::from_utf8_lossy(&out2.stderr),
    );
    // Cargo's stderr should contain `Checking demo_entry` on the
    // second pass — confirms the rebuild fired.
    let stderr2 = String::from_utf8_lossy(&out2.stderr);
    assert!(
        stderr2.contains("Checking demo_entry") || stderr2.contains("Compiling demo_entry"),
        "expected demo_entry to be re-checked after launch.xml touch, but stderr was:\n{stderr2}",
    );
}
