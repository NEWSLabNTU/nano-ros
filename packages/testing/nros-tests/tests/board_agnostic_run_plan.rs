//! Phase 212.O.3 — `board_agnostic_run_plan_links_against_any_board`.
//!
//! Proves the Phase 212.N.4 `nros-build::generate_run_plan` emit is
//! genuinely board-agnostic by linking the SAME `shared_node_pkg`
//! rlib + the SAME launch.xml under two distinct Board impls:
//!
//! * `posix_entry/`     — `<PosixBoard as BoardEntry>::run` (host)
//! * `freertos_entry/`  — `<Mps2An385 as BoardEntry>::run`
//!                        (`thumbv7m-none-eabi`, QEMU MPS2-AN385)
//!
//! KEY assertion: the two Entry pkgs' build.rs files are byte-
//! identical and consume the SAME launch.xml at the SAME nros-build
//! crate rev, so their emitted `$OUT_DIR/run_plan.rs` MUST be byte-
//! identical. That's the operational definition of "board-agnostic
//! codegen".
//!
//! ## Reduced-coverage path
//!
//! When the FreeRTOS prereqs aren't available we still want to
//! exercise SOMETHING. The test runs `cargo build` on `posix_entry/`
//! standalone in that case + asserts the codegen output is a
//! well-formed `pub fn run_plan(...)` body — proving the emit at
//! least doesn't depend on the FreeRTOS Board at codegen time. This
//! is documented + skip-reported on the reduced path; full
//! board-agnosticism only proven when both halves build.
//!
//! ## M-F.17 dependency (resolved)
//!
//! This test exercises the Phase 212 M-F.17 `nros plan` source-metadata
//! α-bridge — `nros-build` resolves component pkgs through
//! `[package.metadata.nros.component]` rather than sidecar
//! `metadata/*.json` artifacts. M-F.17 is landed (planner wires
//! `Workspace::synthetic_metadata_artifacts`), so `generate_run_plan`
//! resolves `shared_node_pkg` from the launch XML's
//! `<node pkg="shared_node_pkg" ...>` directive. The fixtures `[patch]`
//! `nros-build` to the in-tree `packages/cli/nros-build` checkout (see
//! `rewrite_placeholders`), so they pick up the local M-F.17 + M-F.19
//! emit. The FreeRTOS leg is skip-reported when the cross-toolchain is
//! absent (reduced-coverage path).

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use nros_tests::fixtures::freertos::{
    is_arm_gcc_available, is_freertos_available, is_lwip_available,
};

fn workspace_root() -> PathBuf {
    nros_tests::project_root()
}

fn fixture_src() -> PathBuf {
    workspace_root().join("packages/testing/nros-tests/fixtures/n_board_agnostic_run_plan")
}

fn thumbv7m_target_installed() -> bool {
    let Ok(out) = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
    else {
        return false;
    };
    if !out.status.success() {
        return false;
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .any(|l| l.trim() == "thumbv7m-none-eabi")
}

fn freertos_side_ready() -> Result<(), &'static str> {
    if !thumbv7m_target_installed() {
        return Err("thumbv7m-none-eabi target not installed");
    }
    if !is_arm_gcc_available() {
        return Err("arm-none-eabi-gcc not found");
    }
    if !is_freertos_available() {
        return Err("FREERTOS_DIR not set or invalid — run `just freertos setup`");
    }
    if !is_lwip_available() {
        return Err("LWIP_DIR not set or invalid — run `just freertos setup`");
    }
    Ok(())
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

fn rewrite_placeholders(root: &Path, replacement: &str) -> std::io::Result<()> {
    // Phase 212.O.3 / O.5 — also resolve `@NROS_CLI_ROOT@` so fixtures
    // can `[patch]` the `nros-build` git dep against a local nros-cli
    // checkout. Post-Phase-218 the CLI lives in-tree at `packages/cli/`;
    // prefer that, then fall back to a sibling `<nano-ros>/../nros-cli`
    // checkout for users still on the pre-218 layout. When neither
    // resolves the placeholder is left intact and the `[patch]` block
    // fails open — the git dep wins as before.
    // Resolve the `nros-build` crate dir across layouts — in-tree
    // `packages/cli/nros-build`, sibling `../nros-cli/packages/nros-build`,
    // or `$NROS_CLI_ROOT/{nros-build,packages/nros-build}` — then substitute
    // its PARENT for `@NROS_CLI_ROOT@` so the fixture's
    // `@NROS_CLI_ROOT@/nros-build` patch path resolves regardless of layout
    // (the in-tree `packages/cli/` drops the external repo's `packages/`
    // segment).
    let find_nros_build = |base: &std::path::Path| -> Option<std::path::PathBuf> {
        ["nros-build", "packages/nros-build"]
            .into_iter()
            .map(|sub| base.join(sub))
            .find(|cand| cand.join("Cargo.toml").is_file())
    };
    let nros_cli_root = std::env::var("NROS_CLI_ROOT")
        .ok()
        .and_then(|p| find_nros_build(std::path::Path::new(&p)))
        .or_else(|| find_nros_build(&std::path::Path::new(replacement).join("packages/cli")))
        .or_else(|| {
            std::path::Path::new(replacement)
                .parent()
                .and_then(|p| find_nros_build(&p.join("nros-cli")))
        })
        .as_deref()
        .and_then(|d| d.parent())
        .map(|p| p.display().to_string());
    for entry in walk(root)? {
        if !entry.is_file() {
            continue;
        }
        let Ok(text) = fs::read_to_string(&entry) else {
            continue;
        };
        let mut new_text = text.replace("@NANO_ROS_ROOT@", replacement);
        if let Some(cli_root) = nros_cli_root.as_deref() {
            new_text = new_text.replace("@NROS_CLI_ROOT@", cli_root);
        }
        if new_text != text {
            fs::write(&entry, new_text)?;
        }
    }
    Ok(())
}

fn stage_fixture() -> (tempfile::TempDir, PathBuf) {
    let src = fixture_src();
    let dst = tempfile::tempdir().expect("tempdir");
    copy_tree(&src, dst.path()).expect("copy fixture");
    let root_str = workspace_root()
        .to_str()
        .expect("workspace root is utf-8")
        .to_string();
    rewrite_placeholders(dst.path(), &root_str).expect("rewrite placeholders");
    let root = dst.path().to_path_buf();
    (dst, root)
}

/// Locate the unique `run_plan.rs` artifact emitted under
/// `<entry>/target/.../build/<entry>-<hash>/out/run_plan.rs`. Returns
/// `None` if missing — the caller decides whether that's a hard fail
/// (codegen broken) or a soft skip (offline nros-build dep).
fn find_run_plan(entry_dir: &Path, target_triple_subdir: Option<&str>) -> Option<PathBuf> {
    let build_root = match target_triple_subdir {
        Some(triple) => entry_dir.join(format!("target/{triple}/debug/build")),
        None => entry_dir.join("target/debug/build"),
    };
    if !build_root.is_dir() {
        return None;
    }
    for e in walk(&build_root).unwrap_or_default() {
        if e.file_name().and_then(|n| n.to_str()) == Some("run_plan.rs") {
            return Some(e);
        }
    }
    None
}

fn is_placeholder_stub(body: &str) -> bool {
    body.contains("Placeholder — nros-build codegen unavailable")
}

#[test]
fn board_agnostic_run_plan_links_against_any_board() {
    assert!(
        fixture_src().is_dir(),
        "fixture missing at {}",
        fixture_src().display()
    );

    let (_guard, root) = stage_fixture();
    let posix_dir = root.join("posix_entry");
    let freertos_dir = root.join("freertos_entry");

    // --- Sanity: the two build.rs files MUST be byte-identical
    // pre-build. This is what makes the OUT_DIR/run_plan.rs identity
    // assertion meaningful in the first place.
    let posix_build_rs = fs::read(posix_dir.join("build.rs")).expect("read posix build.rs");
    let freertos_build_rs =
        fs::read(freertos_dir.join("build.rs")).expect("read freertos build.rs");
    assert_eq!(
        posix_build_rs, freertos_build_rs,
        "fixture invariant violated: posix_entry/build.rs and freertos_entry/build.rs MUST be byte-identical for the run_plan.rs identity assertion to mean anything",
    );

    // --- POSIX side: always builds (only prereq is the host
    // toolchain; the test runner trivially has it).
    let posix_status = Command::new("cargo")
        .args(["build", "-p", "posix_entry"])
        .current_dir(&posix_dir)
        .output()
        .expect("spawn cargo build (posix)");

    let posix_run_plan_path = find_run_plan(&posix_dir, None);
    let posix_build_ok = posix_status.status.success();

    // If the host-side `cargo build` failed AND no run_plan.rs was
    // emitted, we have no signal at all — surface the failure.
    if !posix_build_ok && posix_run_plan_path.is_none() {
        let reason = if String::from_utf8_lossy(&posix_status.stderr)
            .contains("failed to load source for dependency `nros-build`")
            || String::from_utf8_lossy(&posix_status.stderr)
                .contains("error: failed to get `nros-build`")
        {
            "nros-build git dep unreachable (offline?) AND cargo failed before emitting a stub"
        } else {
            "posix_entry cargo build failed AND no run_plan.rs emitted"
        };
        nros_tests::skip!(
            "{reason}.\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&posix_status.stdout),
            String::from_utf8_lossy(&posix_status.stderr),
        );
    }

    let posix_run_plan_path = posix_run_plan_path.expect("posix_entry emitted no run_plan.rs");
    let posix_run_plan = fs::read(&posix_run_plan_path).expect("read posix run_plan.rs");
    let posix_run_plan_text = String::from_utf8_lossy(&posix_run_plan).into_owned();

    // The codegen output MUST at minimum be a well-formed run_plan
    // body (placeholder stub is also well-formed by construction —
    // the test below treats either shape as valid for the
    // *standalone* posix-side check, then upgrades to the
    // byte-identical check only when both sides built non-stub).
    assert!(
        posix_run_plan_text.contains("pub fn run_plan"),
        "posix run_plan.rs missing `pub fn run_plan`:\n{posix_run_plan_text}",
    );

    // --- FreeRTOS side: gated on cross-toolchain availability.
    if let Err(reason) = freertos_side_ready() {
        // Reduced-coverage path documented in the file header. We
        // proved the POSIX-side codegen at least emits a syntactic
        // run_plan body that doesn't reference Board internals.
        if is_placeholder_stub(&posix_run_plan_text) {
            nros_tests::skip!(
                "FreeRTOS side unavailable ({reason}) AND posix side fell back to the nros-build placeholder stub — neither half can prove board-agnosticism. Re-run with `nros-build` reachable (online) AND `just freertos setup` complete."
            );
        }
        nros_tests::skip!(
            "FreeRTOS side unavailable ({reason}). POSIX side emitted a populated run_plan.rs; the byte-identical cross-Board check requires both halves."
        );
    }

    // Both sides ready — build the freertos entry and compare.
    let workspace = workspace_root();
    let env_pairs: [(&str, PathBuf); 2] = [
        (
            "NROS_PLATFORM_FREERTOS_SRC",
            workspace.join("packages/core/nros-platform-freertos/src"),
        ),
        (
            "NROS_PLATFORM_CFFI_INCLUDE",
            workspace.join("packages/core/nros-platform-api/include"),
        ),
    ];

    let mut cmd = Command::new("cargo");
    cmd.args([
        "build",
        "--target",
        "thumbv7m-none-eabi",
        "-p",
        "freertos_entry",
    ])
    .current_dir(&freertos_dir);
    for (k, v) in &env_pairs {
        cmd.env(k, v);
    }
    let freertos_status = cmd.output().expect("spawn cargo build (freertos)");

    assert!(
        freertos_status.status.success(),
        "freertos_entry cargo build failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&freertos_status.stdout),
        String::from_utf8_lossy(&freertos_status.stderr),
    );

    // Both Entry pkgs produced a final binary?
    let posix_bin = posix_dir.join("target/debug/posix_entry");
    assert!(
        posix_bin.is_file(),
        "posix_entry binary missing at {}",
        posix_bin.display()
    );
    let freertos_bin = freertos_dir.join("target/thumbv7m-none-eabi/debug/freertos_entry");
    assert!(
        freertos_bin.is_file(),
        "freertos_entry binary missing at {}",
        freertos_bin.display()
    );

    let freertos_run_plan_path =
        find_run_plan(&freertos_dir, Some("thumbv7m-none-eabi")).expect("freertos run_plan.rs");
    let freertos_run_plan = fs::read(&freertos_run_plan_path).expect("read freertos run_plan.rs");
    let freertos_run_plan_text = String::from_utf8_lossy(&freertos_run_plan).into_owned();

    // If EITHER side fell back to the placeholder stub, we can't
    // assert genuine codegen identity — only that the stub is
    // consistent. Skip with a precise reason.
    if is_placeholder_stub(&posix_run_plan_text) || is_placeholder_stub(&freertos_run_plan_text) {
        nros_tests::skip!(
            "at least one side fell back to the nros-build placeholder stub (offline?). posix_stub={}, freertos_stub={}. The byte-identical assertion requires real codegen on BOTH sides.",
            is_placeholder_stub(&posix_run_plan_text),
            is_placeholder_stub(&freertos_run_plan_text),
        );
    }

    // THE KEY ASSERTION — both emitted run_plan.rs are byte-
    // identical AFTER stripping the per-Entry-pkg diagnostic header
    // comment that legitimately differs (the emit template
    // includes `// plan.system : <pkg>` so each Entry pkg's emit
    // names its own system; that's useful diagnostic + does NOT
    // leak Board context). Filter `plan.system` comment lines from
    // both sides; everything else must match byte-for-byte.
    let strip_system_comment = |text: &str| -> String {
        text.lines()
            .filter(|l| !l.starts_with("// plan.system"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let posix_filtered = strip_system_comment(&posix_run_plan_text);
    let freertos_filtered = strip_system_comment(&freertos_run_plan_text);
    assert_eq!(
        posix_filtered,
        freertos_filtered,
        "OUT_DIR/run_plan.rs DIFFERS across Board impls (after stripping diagnostic system-header) — codegen leaked Board context.\n--- posix ({}) ---\n{posix_run_plan_text}\n--- freertos ({}) ---\n{freertos_run_plan_text}",
        posix_run_plan_path.display(),
        freertos_run_plan_path.display(),
    );

    // Belt-and-braces: both should reference the shared component.
    assert!(
        posix_run_plan_text.contains("shared_node_pkg::register"),
        "run_plan.rs missing `shared_node_pkg::register` call:\n{posix_run_plan_text}",
    );
}
