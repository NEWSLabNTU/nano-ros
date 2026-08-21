//! `nros::main!()` misuse diagnostics + rebuild-tracking.
//!
//! **These tests compile at run time — the documented exception to "No
//! compilation inside tests" (AGENTS.md / issue 0034).** A compile-*pass* check
//! moves to the build stage as a fixture (see `native_main_macro_forms.rs`),
//! but these cases cannot:
//!   * the three misuse cases must **fail to compile** with a specific
//!     diagnostic — you can't prebuild a build that must fail;
//!   * the rebuild case must run **two** checks across a file touch to prove the
//!     macro's `include_bytes!` stamp forces a re-check.
//!
//! They `cargo check` a staged copy of the `n9_workspace` template directly.
//! The `.config/nextest.toml` timeout-override keeps `native_main_macro_misuse`
//! (a cold check exceeds the 60s default) — this binary is intentionally on
//! that list; the converted positive forms are not.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn fixture_src() -> PathBuf {
    nros_tests::project_root().join("packages/testing/nros-tests/fixtures/n9_workspace")
}

fn stage_fixture() -> (tempfile::TempDir, PathBuf) {
    let src = fixture_src();
    let dst = tempfile::tempdir().expect("tempdir");
    copy_tree(&src, dst.path()).expect("copy fixture");
    let root_str = nros_tests::project_root()
        .to_str()
        .expect("workspace root utf-8")
        .to_string();
    rewrite_placeholders(dst.path(), &root_str).expect("rewrite placeholders");
    stamp_unique_entry_version(dst.path());
    let root = dst.path().to_path_buf();
    (dst, root)
}

/// Issue 0501 — give each staged copy its own `demo_entry` package VERSION.
///
/// Every case stages its own tree but they all share one `CARGO_TARGET_DIR`
/// (see [`shared_check_target_dir`]), and a package is identified by
/// name + version, NOT by the path it was staged to. So the first case to
/// compile `demo_entry` SUCCESSFULLY made every later case's check "fresh":
/// cargo printed `Finished` with no `Checking demo_entry` line and exited 0 —
/// in a suite whose every assertion is "this misuse must FAIL to compile".
///
/// That is why the failing set moved run to run (whichever case won the dir
/// first passed, the rest inherited its artifact) and why any case passed
/// alone. Reproduced deterministically outside nextest: stage two copies, check
/// a VALID one, then check a misuse one against the same target dir — the
/// misuse exits 0.
///
/// A unique version makes them distinct cargo units while leaving the whole
/// dependency graph shared, which is where phase-342 W2's 108.5 s -> 10.3 s
/// came from — the deps, not `demo_entry` itself. Per-case target dirs would
/// also fix it and would give back the cold-build cost that change bought.
fn stamp_unique_entry_version(root: &Path) {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    // Unique per (process, call): nextest runs each case in its own process, so
    // the pid alone would not separate two cases, and the counter alone would
    // not separate two runs sharing the dir.
    let nonce = format!(
        "{}c{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );
    let manifest = root.join("src/demo_entry/Cargo.toml");
    let body = fs::read_to_string(&manifest).expect("read demo_entry manifest");
    // Build metadata (`+…`) — semver-legal, ignored for resolution, and it
    // still makes the package id distinct.
    let stamped = body.replacen(
        "version = \"0.1.0\"",
        &format!("version = \"0.1.0+{nonce}\""),
        1,
    );
    assert_ne!(
        stamped, body,
        "demo_entry manifest no longer declares `version = \"0.1.0\"` — the 0501 \
         uniqueness stamp silently did nothing, and the cases would share one \
         cargo unit again"
    );
    fs::write(&manifest, stamped).expect("write demo_entry manifest");
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

fn check_demo_entry(root: &Path) -> std::process::Output {
    check_demo_entry_with_model_dir(root, None)
}

/// `cargo check` the fixture, optionally pointing the macro at a build-output
/// model directory (`NROS_MODEL_DIR` — the first entry in
/// `nros_orchestration_ir::model_location`'s search order).
/// A target dir SHARED by every case in this binary (phase-342 W2).
///
/// Each case stages its own copy of the fixture into a fresh tempdir, so
/// without this every case also gets a fresh, EMPTY target dir inside that
/// tempdir and pays a cold `cargo check` of the whole graph — the cost this
/// file's own header notes when it says "a cold check exceeds the 60s default".
/// The staged copies differ only in the misuse edit under test, so the
/// dependency graph they compile is identical and one warm dir serves all of
/// them.
///
/// It lives under the build ROOT, NOT in a tempdir, precisely so it survives
/// between runs — a per-run temp dir would be cold every time and this would buy
/// nothing. Via `nros_tests::build_root()` so it follows `NROS_BUILD_ROOT`
/// (phase-334) instead of hardcoding `<repo>/build`.
///
/// Measured, 5 cases at `--test-threads=4`:
///
///     per-case temp target dirs   108.5 s
///     shared dir, first run        38.3 s
///     shared dir, warm             10.3 s      (383 MB)
///
/// Concurrent cargos DO serialize on this dir's lock (phase-340 F3), and that
/// is fine here and only here: the alternative is not parallel warm builds, it
/// is five COLD ones. The lock makes the first case compile while the other four
/// wait, then all four hit a warm cache — which is why even the cold run is 2.8x
/// faster than the status quo it replaces.
fn shared_check_target_dir() -> PathBuf {
    let dir = nros_tests::build_root().join("nros-tests/native_main_macro_misuse-target");
    fs::create_dir_all(&dir).expect("create shared check target dir");
    dir
}

fn check_demo_entry_with_model_dir(root: &Path, model_dir: Option<&Path>) -> std::process::Output {
    let mut cmd = Command::new("cargo");
    cmd.args(["check", "-p", "demo_entry", "--manifest-path"])
        .arg(root.join("Cargo.toml"))
        .env("CARGO_TARGET_DIR", shared_check_target_dir());
    if let Some(d) = model_dir {
        cmd.env("NROS_MODEL_DIR", d);
    }
    cmd.output().expect("spawn cargo check")
}

/// Resolve the fixture's bringup into `model_dir` the way a build does, and
/// return the model's path there.
///
/// phase-330 W4 made the SystemModel a build artifact: no committed
/// `config/system_model.yaml` exists any more, so a test that needs one
/// PRODUCES it (issue 0414). `NROS_MODEL_DIR/<bringup>/<name>` is the location
/// the macro searches first.
fn resolve_fixture_model(root: &Path, model_dir: &Path) -> PathBuf {
    let resolver = nros_tests::launch_resolver_bin().unwrap_or_else(|| {
        nros_tests::skip!("nros-launch-resolve not built (run `just setup-launch-resolve`)")
    });
    let bringup = root.join("src/demo_bringup");
    let out = model_dir.join("demo_bringup").join("system_model.yaml");
    fs::create_dir_all(out.parent().expect("model dir parent")).expect("create model dir");
    let output = Command::new(&resolver)
        .arg(bringup.join("launch/system.launch.xml"))
        .arg("--bringup-root")
        .arg(&bringup)
        .arg("--system")
        .arg(bringup.join("system.toml"))
        .arg("-o")
        .arg(&out)
        .output()
        .expect("spawn nros-launch-resolve");
    assert!(
        output.status.success(),
        "nros-launch-resolve failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    out
}

#[test]
fn custom_tasks_on_owned_spin_emits_error() {
    // `custom_tasks = [...]` is RTIC-only; the `demo_entry` fixture deploys
    // native (OwnedSpin), so it must fail with the documented diagnostic.
    let (_g, root) = stage_fixture();
    fs::write(
        root.join("src/demo_entry/src/main.rs"),
        "nros::main!(custom_tasks = [adc_sample, ui_redraw]);\n",
    )
    .expect("write main.rs");
    let out = check_demo_entry(&root);
    assert!(
        !out.status.success(),
        "expected `cargo check` to fail when `custom_tasks` is used outside RTIC.\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("custom_tasks") && stderr.contains("only valid for the RTIC framework"),
        "expected diagnostic to flag RTIC-only restriction, stderr:\n{stderr}",
    );
}

#[test]
fn custom_tasks_empty_on_owned_spin_still_errors() {
    // Empty list `custom_tasks = []` is also a misuse outside RTIC.
    let (_g, root) = stage_fixture();
    fs::write(
        root.join("src/demo_entry/src/main.rs"),
        "nros::main!(custom_tasks = []);\n",
    )
    .expect("write main.rs");
    let out = check_demo_entry(&root);
    assert!(
        !out.status.success(),
        "expected `cargo check` to fail for `custom_tasks = []` on OwnedSpin.\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("custom_tasks") && stderr.contains("only valid for the RTIC framework"),
        "expected diagnostic to flag RTIC-only restriction, stderr:\n{stderr}",
    );
}

#[test]
fn unknown_board_emits_compile_error() {
    let (_g, root) = stage_fixture();
    // Override Cargo.toml's `deploy = "native"` to an unknown board.
    let cargo_toml = root.join("src/demo_entry/Cargo.toml");
    let raw = fs::read_to_string(&cargo_toml).expect("read demo_entry Cargo.toml");
    let bad = raw.replace("deploy = \"native\"", "deploy = \"frobnicator\"");
    assert_ne!(raw, bad, "expected `deploy = \"native\"` line in fixture");
    fs::write(&cargo_toml, bad).expect("write bad Cargo.toml");

    fs::write(root.join("src/demo_entry/src/main.rs"), "nros::main!();\n").expect("write main.rs");
    let out = check_demo_entry(&root);
    assert!(
        !out.status.success(),
        "expected `cargo check` to fail on unknown board `frobnicator`.\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unknown board"),
        "expected diagnostic mentioning `unknown board`, stderr:\n{stderr}",
    );
}

/// `nros::main!` resolves the model from the bringup's INPUTS when no build
/// system produced one.
///
/// This is the contract issue 0414 left open: the macro used to only LOOK for
/// `config/system_model.yaml` and fail "SystemModel not found" whenever it ran
/// without a build step that resolved first — which a plain `cargo check` of an
/// entry crate always is (entry crates carry no `build.rs`, so they do not even
/// get an `$OUT_DIR` to fall back to).
///
/// No `NROS_MODEL_DIR`, no committed model, no build system: the macro must
/// still compile the entry by resolving `system.toml` + the launch file itself.
#[test]
fn resolves_the_model_from_inputs_without_a_build_system() {
    if nros_tests::launch_resolver_bin().is_none() {
        nros_tests::skip!("nros-launch-resolve not built (run `just setup-launch-resolve`)");
    }
    let (_g, root) = stage_fixture();
    fs::write(
        root.join("src/demo_entry/src/main.rs"),
        "nros::main!(model = \"demo_bringup\");\n",
    )
    .expect("write main.rs");
    // Deliberately NOT passing a model dir — that is the whole point.
    let out = check_demo_entry(&root);
    assert!(
        out.status.success(),
        "the macro must resolve the model from system.toml + launch when nothing \
         produced one:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}

/// Touching the model the BUILD produced must force a re-check.
///
/// This used to read a committed `config/system_model.yaml`; phase-330 W4
/// deleted those, so the test failed on a missing file rather than on the
/// rebuild behaviour (issue 0414). It now resolves the model into a build-output
/// dir and points the macro there — the same path a real build takes — so
/// nothing here depends on a model being committed.
///
/// The touch target is still the model, because that is what the macro tracks.
/// Touching `system.toml` would be the better test of the user-facing contract,
/// and does NOT force a re-check today: `nros::main!` consumes the resolved
/// artifact and never sees the inputs behind it. Recorded on issue 0414 rather
/// than asserted here, because asserting it would be asserting a wish.
#[test]
fn rebuilds_on_model_touch() {
    let (_g, root) = stage_fixture();
    let model_home = tempfile::tempdir().expect("model dir");
    let model_yaml = resolve_fixture_model(&root, model_home.path());
    fs::write(
        root.join("src/demo_entry/src/main.rs"),
        "nros::main!(model = \"demo_bringup\");\n",
    )
    .expect("write main.rs");
    let out = check_demo_entry_with_model_dir(&root, Some(model_home.path()));
    assert!(
        out.status.success(),
        "initial cargo check failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );

    // Touch the resolved model — the macro's tracked-file stamp should force a
    // re-check. Sleep past cargo's fingerprint mtime resolution, then rewrite.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let body = fs::read_to_string(&model_yaml).expect("read resolved model");
    fs::write(&model_yaml, &body).expect("rewrite resolved model");

    let out2 = check_demo_entry_with_model_dir(&root, Some(model_home.path()));
    assert!(
        out2.status.success(),
        "second cargo check (post-touch) failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out2.stdout),
        String::from_utf8_lossy(&out2.stderr),
    );
    let stderr2 = String::from_utf8_lossy(&out2.stderr);
    assert!(
        stderr2.contains("Checking demo_entry") || stderr2.contains("Compiling demo_entry"),
        "expected demo_entry to be re-checked after the model touch, stderr:\n{stderr2}",
    );
}
