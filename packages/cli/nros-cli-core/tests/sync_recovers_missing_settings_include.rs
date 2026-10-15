//! Issue 1920 -- `nros sync` recovers a leaf whose `.cargo/config.toml`
//! includes a generated settings file that is gone.
//!
//! Sync wires `include = [.., "../build/<image>/nros-cargo.toml"]` into a
//! single-package leaf's gitignored `.cargo/config.toml` (issue 1381), and
//! writes that settings file LAST, from the metadata probe's output. The probe
//! used to read every `include` of the leaf's config for its `[patch]` rows,
//! so once the settings file was missing (a removed `build/`, or one image
//! dir) and the probe had to re-run (a source edit), the probe failed, sync
//! failed before the stage that would have rewritten the file, and every later
//! sync failed the same way -- naming `nros sync` as the remedy.
//!
//! This builds exactly that state on a COPY of `examples/linux/rust/listener`
//! (its tracked files, under the repo's gitignored `tmp/`): sync once, delete
//! the settings file, edit a source line so the probe must re-run, sync again.
//! The second sync must succeed and write the settings file back.
//!
//! Run with:
//! `cargo test --manifest-path packages/cli/Cargo.toml --test sync_recovers_missing_settings_include`

mod common;

use std::{
    fs,
    path::{Path, PathBuf},
};

// Issue 1659 -- every git the CLI tree spawns clears the inherited git env.
include!("../../../tooling/nros-build-paths/src/git_env.rs");

use clap::Parser;
use nros_cli_core::cmd::Cmd;

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

fn nros(argv: &[&str]) -> Result<(), String> {
    let mut full = vec!["nros"];
    full.extend_from_slice(argv);
    let cli = Cli::try_parse_from(&full).map_err(|e| e.to_string())?;
    nros_cli_core::run(cli.cmd).map_err(|e| format!("{e:?}"))
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("repo root")
        .to_path_buf()
}

/// The TRACKED files of the in-tree listener leaf, copied under `tmp/` -- never
/// its build output, so the copy starts as a fresh clone would.
fn scratch_leaf() -> PathBuf {
    let root = repo_root();
    let src = Path::new("examples/linux/rust/listener");
    let dir = root.join("tmp").join(format!(
        "sync-1920-{}-{}",
        std::process::id(),
        common::unique_stamp()
    ));
    let leaf = dir.join("listener");
    let out = nros_git_command("git")
        .arg("-C")
        .arg(&root)
        .args(["ls-files", "-z", "--"])
        .arg(src)
        .output()
        .expect("git ls-files");
    assert!(out.status.success(), "git ls-files failed");
    let files: Vec<&[u8]> = out
        .stdout
        .split(|b| *b == 0)
        .filter(|f| !f.is_empty())
        .collect();
    assert!(!files.is_empty(), "the listener leaf is tracked");
    for f in files {
        let rel = Path::new(std::str::from_utf8(f).unwrap());
        let dest = leaf.join(rel.strip_prefix(src).unwrap());
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        fs::copy(root.join(rel), &dest).unwrap();
    }
    leaf
}

#[test]
fn sync_recovers_a_leaf_whose_settings_include_is_missing() {
    common::isolate_model_discovery();
    let resolver = common::pinned_launch_resolver();
    // SAFETY: once, before any verb runs; the readers are this process's own
    // verbs. The stale guard compares the running BINARY's checkout with the
    // workspace's, and this binary is a test, not the `nros` it guards.
    unsafe {
        std::env::set_var("NROS_LAUNCH_RESOLVE", &resolver);
        std::env::set_var("NROS_SKIP_STALE_CHECK", "1");
    }

    let leaf = scratch_leaf();
    let leaf_s = leaf.to_str().unwrap().to_string();
    let settings = leaf.join("build/linux/nros-cargo.toml");
    let config = leaf.join(".cargo/config.toml");

    // CONTROL -- a first sync writes the settings and wires its include.
    nros(&["sync", &leaf_s]).expect("the copied listener syncs as authored");
    assert!(settings.is_file(), "sync wrote {}", settings.display());
    let cfg = fs::read_to_string(&config).unwrap();
    assert!(
        cfg.contains("../build/linux/nros-cargo.toml"),
        "the leaf's config includes the settings file (issue 1381):\n{cfg}"
    );

    let sidecar = leaf.join("metadata/listener.json");
    let probed_before = fs::metadata(&sidecar)
        .and_then(|m| m.modified())
        .expect("the first sync probed the component");

    // THE ABSORBING STATE -- the include target is gone, and the probe must
    // re-run because a source changed.
    fs::remove_file(&settings).unwrap();
    let lib = leaf.join("src/lib.rs");
    let mut body = fs::read_to_string(&lib).unwrap();
    body.push_str("\n// issue 1920: a source edit, so the metadata probe re-runs\n");
    fs::write(&lib, body).unwrap();

    let second = nros(&["sync", &leaf_s]);
    assert!(
        second.is_ok(),
        "sync must recover a leaf whose generated settings include is missing:\n{}",
        second.unwrap_err()
    );
    assert!(
        settings.is_file(),
        "the recovering sync wrote {} back",
        settings.display()
    );

    // The state only exists while the probe has to RE-RUN; a sync that
    // answered from a fresh sidecar would pass without reaching it.
    let probed_after = fs::metadata(&sidecar).unwrap().modified().unwrap();
    assert!(
        probed_after > probed_before,
        "the second sync re-ran the probe (sidecar {})",
        sidecar.display()
    );

    let _ = fs::remove_dir_all(leaf.parent().unwrap());
}
