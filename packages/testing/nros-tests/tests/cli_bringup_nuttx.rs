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
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
};

fn fixture() -> PathBuf {
    nros_tests::fixtures::fixture_dir("multi_pkg_workspace_nuttx")
}

/// Skip-or-proceed guard: returns normally ONLY when every precondition holds.
///
/// Issue 1135 — the third arm used to `return None` and the one caller wrote
/// `if require_nuttx_setup().is_none() { return; }`, so a host with NuttX and
/// arm-gcc but no built `nros` CLI reported this test GREEN. The other two arms
/// already skipped; the odd one out silently passed. The signature carries the
/// fix: with no value to inspect, a caller cannot get the verdict wrong.
fn require_nuttx_setup() {
    if !is_nuttx_available() {
        nros_tests::skip!(
            "NUTTX_DIR unset / NuttX submodule not provisioned — run `just nuttx setup`"
        );
    }
    if !is_arm_gcc_available() {
        nros_tests::skip!("arm-none-eabi-gcc missing — install gcc-arm-none-eabi");
    }
    if !nros_tests::require_nros_cli() {
        nros_tests::skip!("nros CLI not found — run `just setup-cli` + `source ./activate.sh`");
    }
}

#[test]
fn template_files_exist_and_loc_under_budget() {
    let template = nros_tests::project_root().join("integrations/nuttx/apps-external-template");
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

/// A scratch directory shaped like a NuttX apps tree (the marker the staging
/// script checks is a `Make.defs` at its root).
fn scratch_apps_tree(scratch: &tempfile::TempDir) -> PathBuf {
    let apps = scratch.path().join("nuttx-apps");
    fs::create_dir_all(apps.join("external")).expect("mkdir external");
    fs::write(apps.join("Make.defs"), "# scratch apps tree\n").expect("write Make.defs");
    apps
}

fn stage(apps: &std::path::Path, bringup: &std::path::Path, image: Option<&str>) -> Output {
    let staging = nros_tests::project_root().join("scripts/nuttx/stage-external-apps.sh");
    let mut cmd = Command::new("bash");
    cmd.arg(&staging).arg(apps).arg("--bringup").arg(bringup);
    if let Some(id) = image {
        cmd.arg("--image").arg(id);
    }
    cmd.output().expect("spawn stage-external-apps.sh")
}

/// Issue 1396 — the staged pinning fragment carries the IMAGE id, and the
/// `context::` rule turns it into the `--target` that selects that image's
/// block.
///
/// Before this, the template passed a synthesised `--target nuttx`, which names
/// no block in this fixture (its one image is `qemu-armv7a-nuttx`). `--target`
/// takes an unknown id silently — `tier_resolver::derive_target_rtos` answers
/// with the host's key — so the bake read `[tiers.*.posix]` for a NuttX image
/// and nothing said so. Issue 1312 is the same defect two shims over.
///
/// Needs no NuttX and no `nros`: the staging script and the template's own
/// make expansion are the whole surface under test.
#[test]
fn staged_bringup_pins_the_image_the_codegen_target_names() {
    let fixture_bringup = fixture().join("src/demo_bringup");
    assert!(
        fixture_bringup.is_dir(),
        "fixture bringup missing: {}",
        fixture_bringup.display()
    );
    // The id is read off the fixture rather than hardcoded twice: renaming the
    // block must move this test, not silently un-test it.
    let system_toml =
        fs::read_to_string(fixture_bringup.join("system.toml")).expect("read fixture system.toml");
    let image_id = system_toml
        .lines()
        .find_map(|l| l.trim().strip_prefix("[image.")?.strip_suffix(']'))
        .map(|s| s.trim_matches('"').to_string())
        .unwrap_or_else(|| panic!("fixture declares no [image.*]:\n{system_toml}"));
    assert_ne!(
        image_id, "nuttx",
        "this test's premise is that the fixture's image is NOT named after the \
         platform — if it now is, the retired `--target nuttx` would pass by luck"
    );

    let scratch = tempfile::tempdir().expect("tempdir");
    let apps = scratch_apps_tree(&scratch);
    let out = stage(&apps, &fixture_bringup, Some(&image_id));
    assert!(
        out.status.success(),
        "staging with --image {image_id} failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );

    let pinning = fs::read_to_string(apps.join("external/demo_bringup/nros_bringup.mk"))
        .expect("read staged nros_bringup.mk");
    assert!(
        pinning
            .lines()
            .any(|l| l.split_whitespace().collect::<Vec<_>>()
                == ["NROS_BRINGUP_IMAGE", ":=", image_id.as_str()]),
        "nros_bringup.mk does not pin the image `{image_id}`:\n{pinning}"
    );

    // The template must consume the pin. Asserting the variable is spelled in
    // the `--target` position, not merely that the file mentions it: a pin
    // nothing reads is what the old `--target nuttx` already was.
    let makefile =
        fs::read_to_string(apps.join("external/demo_bringup/Makefile")).expect("read Makefile");
    assert!(
        makefile.contains("$(NROS_CODEGEN_TARGET_ARG)"),
        "Makefile does not pass the pinned image to codegen-system:\n{makefile}"
    );
    assert!(
        makefile.contains("--target $(NROS_BRINGUP_IMAGE)"),
        "NROS_CODEGEN_TARGET_ARG does not expand to a --target:\n{makefile}"
    );
    // Non-comment lines only: the template's own comment explains the retired
    // spelling by quoting it, and a prose mention is not an argument.
    let code: String = makefile
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !code.contains("--target nuttx"),
        "the synthesised platform-name target is back:\n{code}"
    );
}

/// The pin is CHECKED, not trusted. An id naming no block would be accepted by
/// `--target` in silence, which is the whole failure mode, so the one place
/// that can compare it against the bringup's `system.toml` must refuse it.
#[test]
fn staging_refuses_an_image_the_bringup_does_not_declare() {
    let scratch = tempfile::tempdir().expect("tempdir");
    let apps = scratch_apps_tree(&scratch);
    let out = stage(&apps, &fixture().join("src/demo_bringup"), Some("nuttx"));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "staging accepted an undeclared image id:\nstdout:\n{}\nstderr:\n{stderr}",
        String::from_utf8_lossy(&out.stdout),
    );
    assert!(
        stderr.contains("names no block"),
        "refusal does not say why:\n{stderr}"
    );
}

#[test]
fn nuttx_qemu_arm_2_component_bringup_builds() {
    require_nuttx_setup();

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

    let staging = nros_tests::project_root().join("scripts/nuttx/stage-external-apps.sh");
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

    // phase-329 W5 — the former "Step 3" ran `make -C <shell> context` against the
    // NuttX tree but explicitly did NOT assert its result ("Surface output for
    // diagnostic value; do not assert success"). A non-asserting compile/make at
    // test time is exactly what the E1 rule (no compilation inside tests) forbids
    // and buys nothing — dropped. This test's contract is the template-shape audit
    // (Steps 1-2 above, the H.2 contract). The `context::` codegen+build pipeline
    // is exercised for real by the nuttx fixture build stage.
}
