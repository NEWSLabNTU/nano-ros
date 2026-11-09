//! RFC-0071 D16 step 1 (phase-545 W3, issue 2295) — `nros build` selects an
//! RMW backend nano-ros does not know about.
//!
//! `packages/testing/nros-tests/fixtures/out-of-tree-rmw` holds a workspace
//! provider, `src/acme_rmw` (`<nano_ros_provides kind="rmw" name="acme"/>`
//! plus an `nros-rmw.toml`), and a bringup whose image is `board = "linux"`,
//! `rmw = "acme"`. Nothing under `packages/` outside the fixture names `acme`,
//! so the only way `nros build` can find it is the provider scan, and the only
//! way the `linux` board (no `rmw-acme` feature) links it is D15's entry path
//! dependency.
//!
//! This drives `nros build`'s own dispatch in-process on a COPY of the
//! fixture's tracked files: the implicit configure (sync, codegen, the
//! model), then `nros build --dry-run`, which generates the entry package and
//! stops before cargo. No test here compiles an image (CLAUDE.md "No
//! compilation inside tests"): the image is compiled by the build stage
//! (`workspace-out-of-tree-rmw` in `examples/fixtures.toml`) and run by
//! `nros-tests/tests/out_of_tree_rmw_e2e.rs`.
//!
//! Run with:
//! `cargo nextest run --manifest-path packages/cli/Cargo.toml -p nros-cli-core --test out_of_tree_rmw_build`

mod common;

use std::{
    fs,
    path::{Path, PathBuf},
};

use clap::Parser;
use nros_cli_core::cmd::Cmd;

const FIXTURE: &str = "packages/testing/nros-tests/fixtures/out-of-tree-rmw";

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

/// The full dispatch, exactly as the binary runs it (lowering included).
fn nros(argv: &[&str]) -> Result<(), String> {
    let mut full = vec!["nros"];
    full.extend_from_slice(argv);
    let cmd = Cli::try_parse_from(&full)
        .unwrap_or_else(|e| panic!("`{}` does not parse: {e}", argv.join(" ")))
        .cmd;
    nros_cli_core::run(cmd).map_err(|e| format!("{e:?}"))
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("repo root")
        .to_path_buf()
}

fn setup() {
    common::isolate_model_discovery();
    let resolver = common::pinned_launch_resolver();
    // SAFETY: the same values every time, set before any verb in this test
    // runs; the readers are this process's own verbs. The stale guard compares
    // the running BINARY's checkout with the workspace's, and this binary is a
    // test, not the `nros` it guards.
    unsafe {
        std::env::set_var("NROS_LAUNCH_RESOLVE", &resolver);
        std::env::set_var("NROS_SKIP_STALE_CHECK", "1");
    }
}

/// The fixture's TRACKED files, copied to `<repo>/tmp/<unique>/out-of-tree-rmw`
/// so the copy starts as a fresh clone would (no `build/`, no `generated/`).
fn scratch_copy() -> PathBuf {
    let root = repo_root();
    let dest = root
        .join("tmp")
        .join(format!(
            "out-of-tree-rmw-{}-{}",
            std::process::id(),
            common::unique_stamp()
        ))
        .join("out-of-tree-rmw");
    let files = common::tracked_files(&root, FIXTURE);
    assert!(
        files
            .iter()
            .any(|f| f.ends_with("src/acme_rmw/nros-rmw.toml")),
        "{FIXTURE} is tracked and holds the acme provider"
    );
    for f in files {
        let rel = f
            .strip_prefix(root.join(FIXTURE))
            .expect("under the fixture");
        let to = dest.join(rel);
        fs::create_dir_all(to.parent().unwrap()).unwrap();
        fs::copy(&f, &to).unwrap();
    }
    dest
}

/// The one generated entry for image `linux`: `build/<coord>/linux_entry`.
/// Found by listing, not spelled, so the coordinate's derivation stays the
/// builder's.
fn generated_entry(ws: &Path) -> PathBuf {
    let found: Vec<PathBuf> = fs::read_dir(ws.join("build"))
        .expect("the build wrote build/")
        .filter_map(|e| e.ok())
        .map(|e| e.path().join("linux_entry"))
        .filter(|d| d.join("Cargo.toml").is_file())
        .collect();
    assert_eq!(
        found.len(),
        1,
        "exactly one generated `linux_entry` under {}: {found:?}",
        ws.join("build").display()
    );
    found.into_iter().next().unwrap()
}

/// D14 + D15: the build resolves `rmw = "acme"` through the workspace's
/// provider scan, and the generated entry links the backend itself (a direct
/// path dependency on the descriptor's crate) and registers it through the
/// `(image)` arm, because the `linux` board has no `rmw-acme` feature.
#[test]
fn nros_build_links_and_registers_a_workspace_rmw_provider() {
    setup();
    let ws = scratch_copy();
    let wss = ws.to_str().unwrap();

    nros_cli_core::cmd::configure::ensure_current(&ws, &[], false, false, "build")
        .unwrap_or_else(|e| panic!("the implicit configure of an `acme` image: {e:?}"));
    nros(&[
        "build",
        "demo_bringup:linux",
        "--dry-run",
        "--workspace",
        wss,
    ])
    .unwrap_or_else(|e| panic!("`nros build` refused the out-of-tree rmw `acme`: {e}"));

    let entry = generated_entry(&ws);
    let manifest_text = fs::read_to_string(entry.join("Cargo.toml")).unwrap();
    let manifest: toml::Table = toml::from_str(&manifest_text).unwrap();
    let deps = manifest["dependencies"].as_table().expect("[dependencies]");

    // The direct path dependency on `[rmw.provides.cargo] crate`, at the
    // provider's own directory (D15).
    let acme = deps
        .get("acme-rmw")
        .and_then(|d| d.as_table())
        .unwrap_or_else(|| panic!("the entry does not depend on `acme-rmw`:\n{manifest_text}"));
    let path = acme["path"].as_str().expect("a path dependency");
    assert_eq!(
        entry.join(path).canonicalize().unwrap(),
        ws.join("src/acme_rmw").canonicalize().unwrap(),
        "`acme-rmw` must point at the provider the scan found:\n{manifest_text}"
    );
    assert_eq!(
        acme.get("default-features").and_then(|v| v.as_bool()),
        Some(false),
        "D15: `default-features = false`:\n{manifest_text}"
    );
    let features: Vec<&str> = acme["features"]
        .as_array()
        .expect("features")
        .iter()
        .filter_map(|f| f.as_str())
        .collect();
    assert!(
        features.contains(&"platform-posix"),
        "D15: the board's `platform_feature`, which the crate declares: {features:?}"
    );

    // The board still builds, and is not asked for a backend it does not have.
    let board = deps
        .get("nros-board-linux")
        .and_then(|d| d.as_table())
        .expect("the linux board crate");
    let board_features: Vec<&str> = board
        .get("features")
        .and_then(|f| f.as_array())
        .map(|a| a.iter().filter_map(|f| f.as_str()).collect())
        .unwrap_or_default();
    assert!(
        !board_features.iter().any(|f| f.starts_with("rmw-")),
        "the board links no backend for an `acme` image: {board_features:?}"
    );

    // The register macro names the backend on the `(image)` arm, which
    // `nros::main!` calls on the ordinary path.
    let main_rs = fs::read_to_string(entry.join("src/main.rs")).unwrap();
    let image_arm = main_rs
        .split("(image) =>")
        .nth(1)
        .unwrap_or_else(|| panic!("no `(image)` arm in the register macro:\n{main_rs}"));
    let image_arm = image_arm.split("};").next().unwrap();
    assert!(
        image_arm.contains("::acme_rmw::register()"),
        "the `(image)` arm must register `acme_rmw`:\n{main_rs}"
    );

    let _ = fs::remove_dir_all(ws.parent().unwrap());
}
