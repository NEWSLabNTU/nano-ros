//! phase-530 W1 (RFC-0120 D2) -- `nros configure`, and `nros sync` as its alias.
//!
//! Configure is the old `nros sync` plus `nros build` stages 1-4, run under a
//! per-project lock, and it records a stamp per image so a rerun with nothing
//! changed is a no-op. Each test drives the verb in-process on a COPY of a
//! tracked example under the repo's gitignored `tmp/`, so the copy starts as a
//! fresh clone would.
//!
//! Run with:
//! `cargo nextest run --manifest-path packages/cli/Cargo.toml -p nros-cli-core --test configure_verb`

mod common;

use std::{
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

// Issue 1659 -- every git the CLI tree spawns clears the inherited git env.
include!("../../../tooling/nros-build-paths/src/git_env.rs");

use clap::Parser;
use nros_cli_core::cmd::{Cmd, configure};

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

fn parse(argv: &[&str]) -> Cmd {
    let mut full = vec!["nros"];
    full.extend_from_slice(argv);
    Cli::try_parse_from(&full)
        .unwrap_or_else(|e| panic!("`{}` does not parse: {e}", argv.join(" ")))
        .cmd
}

/// The full dispatch, exactly as the binary runs it (lowering included).
fn nros(argv: &[&str]) -> Result<(), String> {
    nros_cli_core::run(parse(argv)).map_err(|e| format!("{e:?}"))
}

/// `nros configure <argv>` through the verb's own entry, for its outcome.
fn configure_outcome(argv: &[&str]) -> configure::Outcome {
    let mut full = vec!["configure"];
    full.extend_from_slice(argv);
    let Cmd::Configure(args) = parse(&full) else {
        panic!("`nros configure` did not parse to Cmd::Configure");
    };
    configure::configure(args).unwrap_or_else(|e| panic!("configure failed: {e:?}"))
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("repo root")
        .to_path_buf()
}

/// Process-wide setup every test needs, once.
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

/// The TRACKED files of `src` (repo-relative), copied to `<tmp>/<tag>/<name>`.
fn scratch_copy(src: &str, tag: &str) -> PathBuf {
    let root = repo_root();
    let src = Path::new(src);
    let dir = root.join("tmp").join(format!(
        "configure-{tag}-{}-{}",
        std::process::id(),
        common::unique_stamp()
    ));
    let dest_root = dir.join(src.file_name().unwrap());
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
    assert!(!files.is_empty(), "{} is tracked", src.display());
    for f in files {
        let rel = Path::new(std::str::from_utf8(f).unwrap());
        let dest = dest_root.join(rel.strip_prefix(src).unwrap());
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        fs::copy(root.join(rel), &dest).unwrap();
    }
    dest_root
}

fn scratch_leaf(tag: &str) -> PathBuf {
    scratch_copy("examples/linux/rust/listener", tag)
}

fn stamp_file(project: &Path, key: &str) -> PathBuf {
    project
        .join("build/nros/configured")
        .join(key.replace(':', "__"))
        .join("stamp.toml")
}

fn mtime(p: &Path) -> SystemTime {
    fs::metadata(p)
        .and_then(|m| m.modified())
        .unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

fn cleanup(project: &Path) {
    let _ = fs::remove_dir_all(project.parent().unwrap());
}

#[test]
fn configure_writes_a_stamp_per_image() {
    setup();
    let leaf = scratch_leaf("stamp");
    let out = configure_outcome(&[leaf.to_str().unwrap()]);
    assert!(!out.up_to_date, "a fresh copy has no stamp: {out:?}");
    assert_eq!(out.configured, ["linux"], "{out:?}");
    assert!(
        stamp_file(&leaf, "linux").is_file(),
        "configure recorded {}",
        stamp_file(&leaf, "linux").display()
    );
    // Stage 4 ran: the image's generated settings file exists.
    assert!(leaf.join("build/linux/nros-cargo.toml").is_file());
    cleanup(&leaf);
}

#[test]
fn second_configure_is_a_no_op() {
    setup();
    let leaf = scratch_leaf("noop");
    let leaf_s = leaf.to_str().unwrap();
    configure_outcome(&[leaf_s]);
    let watched = [
        leaf.join("build/linux/nros-cargo.toml"),
        leaf.join("metadata/listener.json"),
        stamp_file(&leaf, "linux"),
    ];
    let before: Vec<SystemTime> = watched.iter().map(|p| mtime(p)).collect();

    let out = configure_outcome(&[leaf_s]);
    assert!(out.up_to_date, "nothing changed, so nothing runs: {out:?}");
    assert!(out.configured.is_empty(), "{out:?}");
    let after: Vec<SystemTime> = watched.iter().map(|p| mtime(p)).collect();
    assert_eq!(before, after, "a no-op configure touched {watched:?}");
    cleanup(&leaf);
}

#[test]
fn system_toml_edit_reconfigures() {
    setup();
    let leaf = scratch_leaf("edit");
    let leaf_s = leaf.to_str().unwrap();
    configure_outcome(&[leaf_s]);
    let recorded = fs::read_to_string(stamp_file(&leaf, "linux")).unwrap();

    let sys = leaf.join("system.toml");
    let body = fs::read_to_string(&sys)
        .unwrap()
        .replace("domain_id = 0", "domain_id = 7");
    fs::write(&sys, body).unwrap();

    let out = configure_outcome(&[leaf_s]);
    assert!(
        !out.up_to_date,
        "an edited system.toml reconfigures: {out:?}"
    );
    assert_eq!(out.configured, ["linux"], "{out:?}");
    let now = fs::read_to_string(stamp_file(&leaf, "linux")).unwrap();
    assert_ne!(recorded, now, "the stamp records the new digest");
    cleanup(&leaf);
}

#[test]
fn sync_alias_runs_configure() {
    setup();
    let leaf = scratch_leaf("alias");
    nros(&["sync", leaf.to_str().unwrap()]).expect("`nros sync` runs configure");
    assert!(
        stamp_file(&leaf, "linux").is_file(),
        "`nros sync` recorded the configure stamp"
    );
    cleanup(&leaf);
}

/// A workspace whose bringup declares a host image and a second image on a
/// board whose Rust target no host has installed: configuring the workspace
/// (no image named) skips the second, and still succeeds.
#[test]
fn sync_alias_succeeds_when_an_image_preflight_refuses() {
    setup();
    let ws = scratch_copy("examples/templates/multi-node-workspace", "skip");
    // A workspace-local board (the catalog absorbs a package's
    // `nros-board.toml`): the host board, but pinned to a Rust target that
    // does not exist, so stage 3 refuses it on every host with rustup.
    let board = ws.join("src/never_provisioned_board");
    fs::create_dir_all(&board).unwrap();
    fs::write(
        board.join("package.xml"),
        "<?xml version=\"1.0\"?>\n<package format=\"3\">\n<name>never_provisioned_board</name>\n\
         <version>0.0.0</version>\n<description>t</description>\n\
         <maintainer email=\"a@b.c\">m</maintainer>\n<license>Apache-2.0</license>\n</package>\n",
    )
    .unwrap();
    fs::write(
        board.join("nros-board.toml"),
        "[[board]]\nnames = [\"never-provisioned\"]\nplatform = \"posix\"\n\
         toolchain = \"stable\"\nentry_kind = \"hosted-main\"\n\
         target = \"nros-never-installed-none-elf\"\nsupported_netstacks = []\n\n\
         [board.net]\nstack = \"host\"\nlinks = [\"tcp\", \"udp\"]\n",
    )
    .unwrap();
    let sys = ws.join("src/demo_bringup/system.toml");
    let mut body = fs::read_to_string(&sys).unwrap();
    body.push_str("\n[image.cross]\nboard = \"never-provisioned\"\n");
    fs::write(&sys, body).unwrap();

    nros(&["sync", ws.to_str().unwrap()])
        .expect("an image the caller did not name is skipped, not an error");
    assert!(stamp_file(&ws, "demo_bringup:linux").is_file());
    assert!(
        !stamp_file(&ws, "demo_bringup:cross").exists(),
        "a skipped image records no stamp"
    );

    // The outcome names the skip and its reason.
    let out = configure_outcome(&[ws.to_str().unwrap()]);
    assert_eq!(out.skipped.len(), 1, "{out:?}");
    assert_eq!(out.skipped[0].0, "demo_bringup:cross", "{out:?}");
    assert!(
        out.skipped[0].1.contains("nros-never-installed-none-elf"),
        "{out:?}"
    );

    // Named, the same refusal is an error.
    let named = nros(&[
        "configure",
        ws.to_str().unwrap(),
        "--image",
        "demo_bringup:cross",
    ]);
    let err = named.expect_err("a named image's preflight refusal is an error");
    assert!(err.contains("nros-never-installed-none-elf"), "{err}");
    cleanup(&ws);
}

#[test]
fn second_concurrent_configure_finds_stamp_current() {
    setup();
    let leaf = scratch_leaf("concurrent");
    let leaf_s = leaf.to_str().unwrap().to_string();
    let threads: Vec<_> = (0..2)
        .map(|_| {
            let l = leaf_s.clone();
            std::thread::spawn(move || configure_outcome(&[&l]))
        })
        .collect();
    let outs: Vec<configure::Outcome> = threads.into_iter().map(|t| t.join().unwrap()).collect();
    let up_to_date = outs.iter().filter(|o| o.up_to_date).count();
    assert_eq!(
        up_to_date, 1,
        "exactly one finds the other's stamp: {outs:?}"
    );
    cleanup(&leaf);
}
