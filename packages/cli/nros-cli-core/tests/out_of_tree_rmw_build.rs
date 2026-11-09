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
    // The copy holds an rmw provider (`acme`), and the tests that scan this
    // whole checkout from disk (`cargo-nano-ros`'s
    // `every_announced_backend_resolves_through_the_scan`) would find it beside
    // the tracked fixture and refuse two claims of `acme` in one root. The
    // provider scan's own ignore marker on the copy's PARENT hides it from a
    // walk that comes down through `tmp/`, and leaves the copy itself — the
    // root this test's build scans from — untouched.
    fs::create_dir_all(dest.parent().unwrap()).unwrap();
    fs::write(dest.parent().unwrap().join("COLCON_IGNORE"), "").unwrap();
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

/// The one generated west application for image `zephyr`:
/// `build/<coord>/zephyr_entry`, found by listing like [`generated_entry`].
fn generated_west_application(ws: &Path) -> PathBuf {
    let found: Vec<PathBuf> = fs::read_dir(ws.join("build"))
        .expect("the build wrote build/")
        .filter_map(|e| e.ok())
        .map(|e| e.path().join("zephyr_entry"))
        .filter(|d| d.join("Cargo.toml").is_file() && d.join("CMakeLists.txt").is_file())
        .collect();
    assert_eq!(
        found.len(),
        1,
        "exactly one generated `zephyr_entry` west application under {}: {found:?}",
        ws.join("build").display()
    );
    found.into_iter().next().unwrap()
}

/// RFC-0071 D16 step 3 (issue 2372) — the same `acme` image on
/// `board = "zephyr"` (native_sim) goes through `builder/west_app.rs`. The
/// west application's backend dependency IS D15's (amended ruling 2): the
/// crate the descriptor names, at `ResolvedRmwIn::package_dir` (ruling 3, the
/// deleted `backend_dir` walk only looked under the SDK), behind the entry's
/// own `rmw-acme` feature that the `(gated)` arm keys on. And the Zephyr
/// module's Kconfig fragment selects the module backend `acme` is built over
/// (`[rmw.zephyr] module_backend = "zenoh"`), never a choice named after it.
///
/// No Zephyr SDK is needed: `--dry-run` writes the application and stops
/// before west, and the Kconfig fragment is the module hook's own CLI verb.
/// The image is compiled by the `workspace-zephyr-out-of-tree-rmw` row on the
/// Zephyr lane and run by `nros-tests/tests/out_of_tree_rmw_zephyr_e2e.rs`.
#[test]
fn nros_build_generates_a_west_application_over_a_workspace_rmw_provider() {
    setup();
    let ws = scratch_copy();
    let wss = ws.to_str().unwrap();

    nros_cli_core::cmd::configure::ensure_current(&ws, &[], false, false, "build")
        .unwrap_or_else(|e| panic!("the implicit configure of an `acme` image: {e:?}"));
    nros(&[
        "build",
        "demo_bringup:zephyr",
        "--dry-run",
        "--offline",
        "--workspace",
        wss,
    ])
    .unwrap_or_else(|e| panic!("`nros build` refused the west `acme` image: {e}"));

    let app = generated_west_application(&ws);
    let manifest_text = fs::read_to_string(app.join("Cargo.toml")).unwrap();
    let manifest: toml::Table = toml::from_str(&manifest_text).unwrap();
    let acme = manifest["dependencies"]
        .get("acme-rmw")
        .and_then(|d| d.as_table())
        .unwrap_or_else(|| {
            panic!("the west application does not depend on `acme-rmw`:\n{manifest_text}")
        });
    let path = acme["path"].as_str().expect("a path dependency");
    assert_eq!(
        app.join(path).canonicalize().unwrap(),
        ws.join("src/acme_rmw").canonicalize().unwrap(),
        "`acme-rmw` must point at the provider's own directory:\n{manifest_text}"
    );
    assert_eq!(
        acme.get("default-features").and_then(|v| v.as_bool()),
        Some(false),
        "{manifest_text}"
    );
    assert_eq!(
        acme.get("optional").and_then(|v| v.as_bool()),
        Some(true),
        "the dep is enabled by the entry's `rmw-acme` feature:\n{manifest_text}"
    );
    let features: Vec<&str> = acme["features"]
        .as_array()
        .expect("features")
        .iter()
        .filter_map(|f| f.as_str())
        .collect();
    assert!(
        features.contains(&"platform-zephyr"),
        "the `zephyr` board's `platform_feature`, which acme declares: {features:?}"
    );
    let feats = manifest["features"].as_table().expect("[features]");
    assert_eq!(
        feats["default"].as_array().unwrap()[0].as_str(),
        Some("rmw-acme"),
        "{manifest_text}"
    );
    assert_eq!(
        feats["rmw-acme"].as_array().unwrap()[0].as_str(),
        Some("dep:acme-rmw"),
        "{manifest_text}"
    );

    // The `(gated)` arm `nros::main!`'s Zephyr arm calls registers acme.
    let lib_rs = fs::read_to_string(app.join("src/lib.rs")).unwrap();
    let gated = lib_rs
        .split("(gated) =>")
        .nth(1)
        .unwrap_or_else(|| panic!("no `(gated)` arm in the register macro:\n{lib_rs}"));
    assert!(
        gated.contains("feature = \"rmw-acme\"") && gated.contains("::acme_rmw::register()"),
        "the `(gated)` arm must register `acme_rmw` behind `rmw-acme`:\n{lib_rs}"
    );

    // The module hook's Kconfig fragment: zenoh, the module backend acme is
    // built over.
    let frag = ws.join("build/kconfig-zephyr.conf");
    let defaults = ws.join("build/kconfig-zephyr-defaults.conf");
    let env = ws.join("build/kconfig-zephyr-env.toml");
    nros(&[
        "dev",
        "image",
        "leaf-system",
        ws.join("src/demo_bringup").to_str().unwrap(),
        "--nano-ros-path",
        repo_root().to_str().unwrap(),
        "--image",
        "zephyr",
        "--language",
        "rust",
        "--kconfig-out",
        frag.to_str().unwrap(),
        "--kconfig-defaults-out",
        defaults.to_str().unwrap(),
        "--image-env-out",
        env.to_str().unwrap(),
    ])
    .unwrap_or_else(|e| panic!("the module hook's Kconfig render refused `acme`: {e}"));
    let fragment = fs::read_to_string(&frag).unwrap();
    assert!(
        fragment.contains("CONFIG_NROS_RMW_ZENOH=y"),
        "an `acme` image selects the module backend it is built over:\n{fragment}"
    );

    let _ = fs::remove_dir_all(ws.parent().unwrap());
}
