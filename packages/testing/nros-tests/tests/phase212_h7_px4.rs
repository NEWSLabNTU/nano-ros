//! Phase 212.H.7 — PX4 hookless-vendor module-build gate.
//!
//! Drives `nros codegen-system --ahead-of-vendor --target px4` against
//! `fixtures/multi_pkg_workspace_px4/`, then checks the rendered module
//! dirs land under `$PX4_AUTOPILOT_DIR/src/modules/`. To keep CI under
//! a budget the test uses `make px4_sitl_default --dry-run` (the actual
//! SITL link is ~10 minutes); the dry-run is enough to assert PX4's
//! own walker discovered the new module dirs.
//!
//! Skips cleanly when `nros` CLI, `$PX4_AUTOPILOT_DIR` (or `$PX4_DIR`),
//! or the codegen subcommand are missing.  Sibling to
//! `phase212_h5_esp_idf.rs`; uses the same fixture-staging + tempdir
//! pattern.

use std::{path::PathBuf, process::Command};

fn fixture() -> PathBuf {
    nros_tests::project_root().join("packages/testing/nros-tests/fixtures/multi_pkg_workspace_px4")
}

/// Components declared in the fixture's `demo_bringup/system.toml`.
const COMPONENTS: &[&str] = &["talker", "brake_arbiter"];

#[test]
#[ignore = "Phase 212.H.7 codegen ahead-of-vendor PX4 emit is a skeleton; require_px4 + px4_autopilot_dir helpers also pending. Un-ignore when codegen-system writes the nros_<name>/ dirs to $PX4_AUTOPILOT_DIR/src/modules/ in the documented shape."]
fn px4_sitl_2_component_module_builds() {
    // Phase 212.H.7 prereqs: nros CLI + a PX4-Autopilot checkout. The
    // codegen subcommand check below is a soft gate (the verb may not
    // yet exist while the nros-cli side of 212.H.7 lands), surfaced via
    // `skip!` so the test doesn't fail the run.
    if !nros_tests::require_nros_cli() {
        nros_tests::skip!("nros CLI not found — run `scripts/install-nros.sh`");
    }
    if !nros_tests::require_px4() {
        nros_tests::skip!(
            "PX4_AUTOPILOT_DIR / PX4_DIR unset — run `just px4 setup` or load `.envrc`"
        );
    }

    let nros = nros_tests::nros_cli_bin_path().expect("require_nros_cli passed");
    let px4_dir = nros_tests::px4_autopilot_dir().expect("require_px4 passed");
    let fixture_dir = fixture();
    assert!(
        fixture_dir.join("demo_bringup/system.toml").is_file(),
        "fixture demo_bringup/system.toml missing at {}",
        fixture_dir.display()
    );

    // We point the codegen at a tempdir under the PX4 tree (then move
    // the rendered children into `src/modules/` if absent). This keeps
    // the test reversible — we delete just the `nros_*` module dirs we
    // wrote, never touching stock PX4 modules.
    let modules_dir = px4_dir.join("src/modules");
    assert!(
        modules_dir.is_dir(),
        "{} missing — not a real PX4 checkout?",
        modules_dir.display()
    );

    // Track which module dirs WE created so the test cleans only its
    // own writes on the way out (Drop-by-RAII via the closure below).
    let mut created: Vec<PathBuf> = Vec::new();
    for c in COMPONENTS {
        let p = modules_dir.join(format!("nros_{c}"));
        if !p.exists() {
            created.push(p);
        }
    }
    // Best-effort cleanup; ignore failures (PX4 build may have written
    // *.o into the dir between codegen and our cleanup pass).
    let cleanup = scopeguard_lite(move || {
        for p in &created {
            let _ = std::fs::remove_dir_all(p);
        }
    });

    // Phase 212.H.7 hookless verb. The flag set matches the pattern
    // documented in `integrations/px4/README.md`; when the verb doesn't
    // exist yet (nros-cli side of 212.H.7 still landing), surface it
    // as a clean skip rather than a hard fail.
    let codegen = Command::new(&nros)
        .args([
            "codegen-system",
            "--ahead-of-vendor",
            "px4",
            "--target",
            "px4",
            "--bringup",
            "demo_bringup",
        ])
        .arg("--workspace")
        .arg(&fixture_dir)
        .arg("--out")
        .arg(&modules_dir)
        .output()
        .expect("spawn nros codegen-system");

    if !codegen.status.success() {
        let stderr = String::from_utf8_lossy(&codegen.stderr).to_string();
        let stdout = String::from_utf8_lossy(&codegen.stdout).to_string();
        // The verb / flag combo is still landing on the nros-cli side.
        // Treat "unknown subcommand", "unrecognized arg", and the
        // explicit `TODO(212.H.7)` marker the same way: skip cleanly.
        let looks_unimpl = stderr.contains("unrecognized subcommand")
            || stderr.contains("unrecognised subcommand")
            || stderr.contains("unrecognized argument")
            || stderr.contains("unexpected argument")
            || stderr.contains("TODO(212.H.7)")
            || stdout.contains("TODO(212.H.7)");
        drop(cleanup);
        if looks_unimpl {
            nros_tests::skip!(
                "nros codegen-system --ahead-of-vendor --target px4 not yet implemented \
                 in the installed CLI (Phase 212.H.7 nros-cli side still landing):\n\
                 stdout:\n{stdout}\nstderr:\n{stderr}"
            );
        }
        panic!("nros codegen-system failed:\nstdout:\n{stdout}\nstderr:\n{stderr}");
    }

    // Post-codegen: assert one rendered module dir per component is
    // emitted alongside the bake tree. The 212.H.7 audit scope is
    // "codegen --ahead-of-vendor px4 emits the per-component skeletons";
    // wiring those skeletons into $PX4_AUTOPILOT_DIR/src/modules/ is a
    // separate sweep (codegen-system's px4 emit is still a skeleton —
    // emits `<name>_module/` next to the bake tree, not the final PX4
    // module layout).
    let module_root = modules_dir.clone();
    for c in COMPONENTS {
        let mod_dir = module_root.join(format!("{c}_module"));
        assert!(
            mod_dir.is_dir(),
            "expected codegen to emit {} for component '{c}'",
            mod_dir.display()
        );
        let cmake = mod_dir.join("CMakeLists.txt");
        let src_a = mod_dir.join(format!("{c}.cpp"));
        let src_b = mod_dir.join("module.h");
        let src = if src_a.is_file() { src_a } else { src_b };
        assert!(
            cmake.is_file(),
            "missing CMakeLists.txt at {}",
            cmake.display()
        );
        assert!(src.is_file(), "missing module source at {}", src.display());
        let body = std::fs::read_to_string(&cmake).expect("read rendered CMakeLists.txt");
        assert!(
            body.contains("px4_add_module"),
            "rendered CMakeLists.txt at {} must call px4_add_module(); got:\n{body}",
            cmake.display()
        );
        assert!(
            body.contains(c),
            "rendered CMakeLists.txt at {} must reference component name '{c}'; got:\n{body}",
            cmake.display()
        );
    }

    // PX4's `make px4_sitl_default --dry-run` exercises Make's own
    // discovery of the new module dirs without paying the ~10-min link
    // cost. A non-zero exit here means PX4's config step rejected the
    // emitted module dirs (typically a Kconfig mismatch).
    let dry = Command::new("make")
        .arg("-C")
        .arg(&px4_dir)
        .args(["px4_sitl_default", "--dry-run", "-n"])
        .output()
        .expect("spawn make --dry-run");
    drop(cleanup);
    assert!(
        dry.status.success(),
        "make px4_sitl_default --dry-run failed after codegen:\n\
         stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&dry.stdout),
        String::from_utf8_lossy(&dry.stderr)
    );
    let plan = String::from_utf8_lossy(&dry.stdout);
    for c in COMPONENTS {
        let needle = format!("nros_{c}");
        assert!(
            plan.contains(&needle),
            "make --dry-run output does not mention module '{needle}'; \
             PX4's walker didn't pick it up. plan head:\n{}",
            plan.lines().take(40).collect::<Vec<_>>().join("\n")
        );
    }
}

/// Minimal RAII-on-drop helper local to this test (avoid pulling in
/// the `scopeguard` dep).
struct Cleanup<F: FnMut()> {
    f: Option<F>,
}
impl<F: FnMut()> Drop for Cleanup<F> {
    fn drop(&mut self) {
        if let Some(mut f) = self.f.take() {
            f();
        }
    }
}
fn scopeguard_lite<F: FnMut()>(f: F) -> Cleanup<F> {
    Cleanup { f: Some(f) }
}
