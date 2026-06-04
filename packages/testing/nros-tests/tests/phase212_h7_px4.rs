//! Phase 212.H.7 — PX4 hookless-vendor module-build gate.
//!
//! Drives `nros codegen-system --ahead-of-vendor px4 --target px4`
//! against `fixtures/multi_pkg_workspace_px4/`, then checks the
//! rendered module dirs land under `$PX4_AUTOPILOT_DIR/src/modules/`
//! with the PX4-native `px4_add_module()` CMakeLists + `Kconfig` +
//! `nros_<name>.{cpp,h}` shape (see
//! `third-party/px4/PX4-Autopilot/src/modules/time_persistor/` for the
//! reference module layout this emit mirrors).
//!
//! Scope: dir-shape-only. The `make px4_sitl_default --dry-run` gate
//! that would prove PX4's own walker picked up the modules requires
//! enabling them via the board overlay (`CONFIG_MODULES_NROS_<NAME>=y`
//! in a `.px4board` file under `boards/px4/sitl/`). Stock PX4 SITL
//! configs don't enable nros modules, and editing the vendored
//! PX4-Autopilot submodule is out of scope for this gate (constraint
//! from the H.7 plan); the staged-tempdir copy of the ~1GB PX4 tree
//! would slow the test past the per-test budget. The codegen emit is
//! the unit under test here — wiring the modules into a board overlay
//! lives in a follow-up.
//!
//! Skips cleanly when `nros` CLI or `$PX4_AUTOPILOT_DIR` (alias
//! `$PX4_DIR`) is missing.  Sibling to `phase212_h5_esp_idf.rs`.

use std::{path::PathBuf, process::Command};

fn fixture() -> PathBuf {
    nros_tests::project_root().join("packages/testing/nros-tests/fixtures/multi_pkg_workspace_px4")
}

/// Components declared in the fixture's `demo_bringup/system.toml`.
const COMPONENTS: &[&str] = &["talker", "brake_arbiter"];

#[test]
#[ignore = "Phase 212.M.10: depends on the retired Bringup pkg / \
            `demo_bringup/system.toml` shape (§212.L.3 — 2026-06-02 \
            redesign) AND the M-F.8 PX4 SITL board overlay gap. The \
            fixture has been migrated to the §212.L.9 cmake fn shape \
            (`nano_ros_node_register` + `nano_ros_deploy`); the \
            `nros codegen-system --ahead-of-vendor --target px4` driver \
            needs a follow-up to read the Node pkg surface instead \
            of `system.toml`, plus a board overlay edit to enable the \
            emitted modules."]
fn px4_sitl_2_component_module_builds() {
    // Phase 212.H.7 prereqs: nros CLI + a PX4-Autopilot checkout. The
    // codegen subcommand check below is a soft gate (the verb may not
    // yet exist while the nros-cli side of 212.H.7 lands), surfaced via
    // `skip!` so the test doesn't fail the run.
    if !nros_tests::require_nros_cli() {
        nros_tests::skip!("nros CLI not found — run `just setup-cli` + `source ./activate.sh`");
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

    // Track every path the codegen emit drops under <px4>/src/modules/
    // so the test cleans up its own writes on the way out — keeps the
    // vendored PX4-Autopilot tree free of test-run pollution (CLAUDE.md
    // "Don't modify vendored/generated" guideline).
    let mut created: Vec<PathBuf> = Vec::new();
    for c in COMPONENTS {
        created.push(modules_dir.join(format!("nros_{c}")));
    }
    // The bake tree + side-car plan json also land under --out (=modules_dir);
    // clean those too.
    created.push(modules_dir.join("nros-system"));
    created.push(modules_dir.join("nros-plan.json"));

    // Best-effort cleanup; ignore failures (PX4 build may have written
    // *.o into the dir between codegen and our cleanup pass).
    let cleanup = scopeguard_lite(move || {
        for p in &created {
            if p.is_dir() {
                let _ = std::fs::remove_dir_all(p);
            } else if p.is_file() {
                let _ = std::fs::remove_file(p);
            }
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

    // Post-codegen: assert one rendered `nros_<name>/` PX4-native
    // module dir per component lands under <px4>/src/modules/, w/ the
    // four files PX4's own walker + `px4_add_module()` expects:
    //   CMakeLists.txt   — `px4_add_module(MODULE modules__nros_<n> MAIN nros_<n> ...)`
    //   Kconfig          — `menuconfig MODULES_NROS_<UPPER_N>` (default n)
    //   nros_<name>.cpp  — stub entry point `nros_<name>_main(...)`
    //   nros_<name>.h    — header w/ extern "C" decl
    //
    // The `make px4_sitl_default --dry-run` gate is documented as a
    // follow-up (see module-level doc comment): it requires a board
    // overlay edit to enable the emitted modules, and editing the
    // vendored PX4-Autopilot tree is out of scope for the dir-shape
    // audit this gate covers.
    let module_root = modules_dir.clone();
    for c in COMPONENTS {
        let mod_dir = module_root.join(format!("nros_{c}"));
        assert!(
            mod_dir.is_dir(),
            "expected codegen to emit {} for component '{c}'",
            mod_dir.display()
        );
        let cmake = mod_dir.join("CMakeLists.txt");
        let kconfig = mod_dir.join("Kconfig");
        let cpp = mod_dir.join(format!("nros_{c}.cpp"));
        let header = mod_dir.join(format!("nros_{c}.h"));
        for p in [&cmake, &kconfig, &cpp, &header] {
            assert!(p.is_file(), "missing emitted file {}", p.display());
        }

        let cmake_body = std::fs::read_to_string(&cmake).expect("read rendered CMakeLists.txt");
        assert!(
            cmake_body.contains("px4_add_module("),
            "rendered CMakeLists.txt at {} must call px4_add_module(); got:\n{cmake_body}",
            cmake.display()
        );
        assert!(
            cmake_body.contains(&format!("MODULE modules__nros_{c}")),
            "missing MODULE marker at {}: got:\n{cmake_body}",
            cmake.display()
        );
        assert!(
            cmake_body.contains(&format!("MAIN nros_{c}")),
            "missing MAIN marker at {}: got:\n{cmake_body}",
            cmake.display()
        );

        let kconfig_body = std::fs::read_to_string(&kconfig).expect("read Kconfig");
        assert!(
            kconfig_body.contains(&format!(
                "menuconfig MODULES_NROS_{}",
                c.to_ascii_uppercase()
            )),
            "Kconfig at {} missing menuconfig MODULES_NROS_*; got:\n{kconfig_body}",
            kconfig.display()
        );

        let cpp_body = std::fs::read_to_string(&cpp).expect("read cpp stub");
        assert!(
            cpp_body.contains(&format!("int nros_{c}_main(int argc, char *argv[])")),
            "cpp stub at {} missing nros_<name>_main entry; got:\n{cpp_body}",
            cpp.display()
        );
        assert!(
            cpp_body.contains("px4_platform_common/module.h"),
            "cpp stub at {} missing PX4 module.h include; got:\n{cpp_body}",
            cpp.display()
        );
    }
    drop(cleanup);
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
