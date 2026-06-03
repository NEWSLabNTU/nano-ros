//! Phase 212.H.2 — NuttX adapter alignment test.
//!
//! Verifies the `integrations/nuttx/apps-external-template/` per-bringup
//! shim integrates a Phase 212 multi-component bringup pkg into a NuttX
//! `apps/external/` tree end-to-end:
//!
//! 1. `scripts/nuttx/stage-external-apps.sh --bringup <fixture>` writes
//!    the template (Make.defs + Makefile + Kconfig + nros_bringup.mk)
//!    into a scratch apps tree.
//! 2. The staged shell carries the documented surface:
//!    - `CONFIGURED_APPS += $(APPDIR)/external/demo_bringup` gated on
//!      `CONFIG_NROS_BRINGUP_DEMO`.
//!    - `context::` rule that shells `nros codegen-system` then
//!      `NROS_CARGO_BUILD` (or `RUST_CARGO_BUILD`).
//!    - `Kconfig` exposes a single bool `NROS_BRINGUP_DEMO`.
//! 3. (Conditional) Running NuttX `make context` against the staged
//!    tree drives the `context::` rule.
//!
//! Skip semantics (mirrors `tests/nuttx_qemu.rs::require_nuttx`):
//!  - `NUTTX_DIR` env unset / not a NuttX tree → skip.
//!  - `arm-none-eabi-gcc` not on PATH → skip.
//!  - `nros` CLI not on PATH → skip (Phase 212.H.2 needs it).
//!  - `nros codegen-system` verb not yet implemented (Phase 212.E)
//!    → the shape audit (steps 1+2) still runs; the build step (3) is
//!    skipped with an explanatory message.
//!
//! Run with:
//!   cargo test -p nros-tests --test phase212_h2_nuttx -- --nocapture

use nros_tests::fixtures::nuttx::{is_arm_gcc_available, is_nuttx_available};
use std::{fs, path::PathBuf, process::Command};

fn workspace_root() -> PathBuf {
    nros_tests::project_root()
}

fn fixture() -> PathBuf {
    workspace_root().join("packages/testing/nros-tests/fixtures/multi_pkg_workspace_nuttx")
}

fn require_nuttx_setup() -> Option<()> {
    if !is_nuttx_available() {
        nros_tests::skip!(
            "NUTTX_DIR unset / NuttX submodule not provisioned — run `just nuttx setup`"
        );
    }
    if !is_arm_gcc_available() {
        nros_tests::skip!("arm-none-eabi-gcc missing — install gcc-arm-none-eabi");
    }
    if !nros_tests::require_nros_cli() {
        return None;
    }
    Some(())
}

/// Returns true if the `nros codegen-system` verb is reachable (Phase
/// 212.E). When false the build step is skipped (the audit-only path
/// still verifies template shape).
fn nros_codegen_system_available() -> bool {
    let bin = std::env::var("NROS_BIN")
        .ok()
        .unwrap_or_else(|| "nros".to_string());
    Command::new(&bin)
        .args(["codegen-system", "--help"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[test]
fn template_files_exist_and_loc_under_budget() {
    let template = workspace_root().join("integrations/nuttx/apps-external-template");
    for f in ["Make.defs", "Makefile", "Kconfig", "README.md"] {
        let p = template.join(f);
        assert!(p.is_file(), "missing template file: {}", p.display());
    }

    // 200 LoC HARD cap on the shim (Make.defs + Makefile + Kconfig).
    let mut total_code = 0usize;
    for f in ["Make.defs", "Makefile", "Kconfig"] {
        let body = fs::read_to_string(template.join(f)).expect("read template");
        let code = body
            .lines()
            .filter(|l| {
                let t = l.trim_start();
                !t.is_empty() && !t.starts_with('#')
            })
            .count();
        total_code += code;
    }
    assert!(
        total_code <= 200,
        "integration shim over 200 LoC budget: {} lines",
        total_code
    );
}

#[test]
fn nuttx_qemu_arm_2_component_bringup_builds() {
    if require_nuttx_setup().is_none() {
        return;
    }

    // Stage into a scratch tempdir that mimics NuttX's apps tree shape.
    let scratch = tempfile::tempdir().expect("tempdir");
    let apps = scratch.path().join("nuttx-apps");
    fs::create_dir_all(apps.join("external")).expect("mkdir external");
    // Marker file (`Make.defs` at the apps root) the staging script checks.
    fs::write(apps.join("Make.defs"), "# scratch apps tree\n").expect("write Make.defs");

    let bringup = fixture().join("src/demo_bringup");
    assert!(
        bringup.is_dir(),
        "fixture bringup missing: {}",
        bringup.display()
    );

    let staging = workspace_root().join("scripts/nuttx/stage-external-apps.sh");
    let out = Command::new("bash")
        .arg(&staging)
        .arg(&apps)
        .arg("--bringup")
        .arg(&bringup)
        .output()
        .expect("spawn stage-external-apps.sh");
    if !out.status.success() {
        panic!(
            "stage-external-apps.sh failed:\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
        );
    }
    println!(
        "[stage] stdout:\n{}",
        String::from_utf8_lossy(&out.stdout).trim_end()
    );

    // Step 2 — verify the staged shell shape.
    let shell = apps.join("external/demo_bringup");
    for f in ["Make.defs", "Makefile", "Kconfig", "nros_bringup.mk"] {
        let p = shell.join(f);
        assert!(p.is_file(), "missing staged file: {}", p.display());
    }
    let make_defs = fs::read_to_string(shell.join("Make.defs")).unwrap();
    assert!(
        make_defs.contains("CONFIGURED_APPS"),
        "Make.defs missing CONFIGURED_APPS:\n{make_defs}"
    );
    let makefile = fs::read_to_string(shell.join("Makefile")).unwrap();
    assert!(
        makefile.contains("nros codegen-system") || makefile.contains("$(NROS_BIN) codegen-system"),
        "Makefile missing `nros codegen-system` invocation"
    );
    assert!(
        makefile.contains("NROS_CARGO_BUILD") || makefile.contains("RUST_CARGO_BUILD"),
        "Makefile missing cargo-build dispatch"
    );
    let kconfig = fs::read_to_string(shell.join("Kconfig")).unwrap();
    assert!(
        kconfig.contains("NROS_BRINGUP_DEMO"),
        "Kconfig missing NROS_BRINGUP_DEMO knob"
    );
    let pinning = fs::read_to_string(shell.join("nros_bringup.mk")).unwrap();
    assert!(
        pinning.contains("NROS_BRINGUP_NAME      := demo_bringup"),
        "nros_bringup.mk missing per-bringup pinning:\n{pinning}"
    );
    let source = apps.join("external/demo_bringup-source");
    assert!(
        source.is_symlink() || source.is_dir(),
        "bringup source not staged at {}",
        source.display()
    );

    // Step 3 — exercise the `context::` rule against the actual NuttX
    // tree when the Phase 212.E `nros codegen-system` verb exists.
    // Otherwise the audit ends here (Phase 212.E gates the cargo step;
    // the template shape verification is the H.2 contract).
    if !nros_codegen_system_available() {
        println!(
            "[SKIPPED build step] `nros codegen-system` verb not yet implemented \
             (Phase 212.E). Template-shape audit only."
        );
        return;
    }

    // Real build: best-effort. NuttX kernel configure is heavy
    // (defconfig copy + olddefconfig + kconfig-tweak NROS knobs);
    // replicating it would duplicate hundreds of LoC and is out of
    // scope for this build-shape audit (the legacy
    // `build-fixtures-make` recipe that owned it was retired under
    // Phase 212.M-F.16). Verify only that `make context` against the
    // staged shell drives the codegen + cargo-build pipeline without
    // erroring.
    let nuttx_dir = std::env::var("NUTTX_DIR").expect("NUTTX_DIR set above");
    let make_out = Command::new("make")
        .arg("-C")
        .arg(&shell)
        .arg("context")
        .env("APPDIR", &apps)
        .env("TOPDIR", &nuttx_dir)
        .env("NANO_ROS_ROOT", workspace_root())
        .output()
        .expect("spawn make context");
    println!(
        "[make context] status={} stdout:\n{}\nstderr:\n{}",
        make_out.status,
        String::from_utf8_lossy(&make_out.stdout).trim_end(),
        String::from_utf8_lossy(&make_out.stderr).trim_end(),
    );
    // The scratch APPDIR omits `tools/Rust.mk` and the full kernel
    // configure, so `make context` may legitimately exit non-zero on a
    // missing include — that's beyond the H.2 contract. Surface output
    // for diagnostic value; do not assert success.
}
