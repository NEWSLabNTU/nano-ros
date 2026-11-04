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
///
/// `cargo test` (the `just ci gate` lane) runs these tests as THREADS of one
/// process, in parallel. Configure writes nothing they share into the process
/// environment (Ruling R45): the interface search path is an argument and the
/// held marker is handed to the one child that needs it.
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

/// A copy of `multi-node-workspace` plus an image whose preflight refuses on
/// every host with rustup: a workspace-local board (the catalog absorbs a
/// package's `nros-board.toml`) — the host board, pinned to a Rust target that
/// does not exist.
fn workspace_with_an_unprovisioned_image(tag: &str) -> PathBuf {
    let ws = scratch_copy("examples/templates/multi-node-workspace", tag);
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
    ws
}

/// A workspace whose bringup declares a host image and a second image on a
/// board whose Rust target no host has installed: configuring the workspace
/// (no image named) skips the second, and still succeeds.
#[test]
fn sync_alias_succeeds_when_an_image_preflight_refuses() {
    setup();
    let ws = workspace_with_an_unprovisioned_image("skip");
    nros(&["sync", ws.to_str().unwrap()])
        .expect("an image the caller did not name is skipped, not an error");
    assert!(stamp_file(&ws, "demo_bringup:linux").is_file());
    // Ruling R15: a skipped image records a SKIP stamp (digest + reason).
    let skip = fs::read_to_string(stamp_file(&ws, "demo_bringup:cross"))
        .expect("a skipped image records a skip stamp");
    assert!(skip.contains("skipped"), "{skip}");

    // The outcome names the skip and its reason.
    let out = configure_outcome(&[ws.to_str().unwrap(), "--force"]);
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

/// The env var naming the leaf a [`configure_child`] process configures.
const CHILD_LEAF: &str = "NROS_CONFIGURE_TEST_CHILD_LEAF";
/// The line a [`configure_child`] prints its outcome on.
const CHILD_MARK: &str = "CONFIGURE-CHILD up_to_date=";

/// M2 (phase-530 W1 final review): two configures of one leaf at once, as
/// two PROCESSES. In one process the two shared the environment, so one
/// could read the other's exported `NROS_CONFIGURE_HELD` and the "exactly
/// one finds the stamp current" count raced. Each child is this test binary
/// re-run on [`configure_child`] alone, with its own environment.
#[test]
fn second_concurrent_configure_finds_stamp_current() {
    let leaf = scratch_leaf("concurrent");
    let children: Vec<std::process::Child> = (0..2)
        .map(|_| {
            std::process::Command::new(std::env::current_exe().expect("this test binary"))
                .args(["configure_child", "--exact", "--ignored", "--nocapture"])
                .env(CHILD_LEAF, &leaf)
                .env_remove("NROS_CONFIGURE_HELD")
                .stdout(std::process::Stdio::piped())
                .spawn()
                .expect("spawn a configure child")
        })
        .collect();
    let outs: Vec<String> = children
        .into_iter()
        .map(|c| {
            let out = c.wait_with_output().expect("a configure child ran");
            let text = String::from_utf8_lossy(&out.stdout).into_owned();
            assert!(out.status.success(), "a configure child failed:\n{text}");
            text
        })
        .collect();
    let up_to_date: Vec<bool> = outs
        .iter()
        .map(|t| {
            let line = t
                .lines()
                .find_map(|l| l.strip_prefix(CHILD_MARK))
                .unwrap_or_else(|| panic!("no `{CHILD_MARK}` line in:\n{t}"));
            line.trim() == "true"
        })
        .collect();
    assert_eq!(
        up_to_date.iter().filter(|u| **u).count(),
        1,
        "exactly one finds the other's stamp: {up_to_date:?}"
    );
    cleanup(&leaf);
}

/// The child half of [`second_concurrent_configure_finds_stamp_current`]:
/// configure the leaf [`CHILD_LEAF`] names and print the outcome. Run alone
/// (no [`CHILD_LEAF`]), it configures a scratch leaf of its own, so it never
/// passes having done nothing.
#[test]
#[ignore = "a child process of second_concurrent_configure_finds_stamp_current"]
fn configure_child() {
    setup();
    let own = std::env::var_os(CHILD_LEAF).is_none();
    let leaf = match std::env::var_os(CHILD_LEAF) {
        Some(l) => PathBuf::from(l),
        None => scratch_leaf("child"),
    };
    let out = configure_outcome(&[leaf.to_str().unwrap()]);
    println!("{CHILD_MARK}{}", out.up_to_date);
    if own {
        assert_eq!(out.configured, ["linux"], "{out:?}");
        cleanup(&leaf);
    }
}

/// phase-530 W1 (I1): an OUTPUT configure wrote is gone and every input is
/// unchanged. The stamp used to say "up to date", and the next `nros build`
/// then failed with an internal error.
/// Ruling R43: `nros build --dry-run` never configures, so it writes
/// nothing. A stale stamp is reported ("would configure"); the plan is made
/// from the project as it stands. Before this, a dry run ran C1 -- message
/// codegen, models, the lock seed -- and only skipped recording the stamp.
#[test]
fn dry_run_build_on_a_stale_project_writes_no_stamp_and_no_generated() {
    setup();
    let leaf = scratch_leaf("dry-run-stale");
    let leaf_s = leaf.to_str().unwrap();
    configure_outcome(&[leaf_s]);
    let stamp = stamp_file(&leaf, "linux");
    let recorded = mtime(&stamp);
    // Stale: an output configure wrote is gone.
    fs::remove_dir_all(leaf.join("generated")).unwrap();

    let _ = nros(&["build", "--dry-run", "--workspace", leaf_s]);
    assert!(
        !leaf.join("generated").exists(),
        "a dry run regenerated `generated/`"
    );
    assert_eq!(mtime(&stamp), recorded, "a dry run rewrote the stamp");
    cleanup(&leaf);
}

/// Ruling R45: configure takes the interface search path as an argument and
/// never writes it into the process environment. It exported it before, and
/// on the gate's `cargo test` every configure on another thread then read
/// the last project's path (issue: inputs changed with nothing edited).
#[test]
fn configure_leaves_the_interface_search_path_env_alone() {
    setup();
    let leaf = scratch_leaf("env-untouched");
    let before = std::env::var_os("NROS_INTERFACE_SEARCH_PATH");
    configure_outcome(&[leaf.to_str().unwrap()]);
    assert_eq!(
        std::env::var_os("NROS_INTERFACE_SEARCH_PATH"),
        before,
        "configure wrote NROS_INTERFACE_SEARCH_PATH into the process env"
    );
    cleanup(&leaf);
}

#[test]
fn deleted_generated_dir_reconfigures() {
    setup();
    let leaf = scratch_leaf("deleted-generated");
    let leaf_s = leaf.to_str().unwrap();
    configure_outcome(&[leaf_s]);
    let std_msgs = leaf.join("generated/std_msgs/Cargo.toml");
    assert!(std_msgs.is_file(), "configure generated std_msgs");
    fs::remove_dir_all(leaf.join("generated")).unwrap();

    let out = configure_outcome(&[leaf_s]);
    assert!(!out.up_to_date, "a deleted `generated/` is stale: {out:?}");
    assert_eq!(out.configured, ["linux"], "{out:?}");
    assert!(std_msgs.is_file(), "configure regenerated std_msgs");
    let again = configure_outcome(&[leaf_s]);
    assert!(again.up_to_date, "and is current after: {again:?}");
    cleanup(&leaf);
}

/// phase-530 W1 (I2): `--ros-edition` changes what C1 generates, so a
/// configure with it is not the configure the stamp recorded. (`humble` is
/// the image's own edition: the flag's VALUE is what the stamp keys on.)
#[test]
fn ros_edition_change_reconfigures() {
    setup();
    let leaf = scratch_leaf("edition-flag");
    let leaf_s = leaf.to_str().unwrap();
    let first = configure_outcome(&[leaf_s]);
    assert_eq!(first.configured, ["linux"], "{first:?}");
    let out = configure_outcome(&[leaf_s, "--ros-edition", "humble"]);
    assert!(!out.up_to_date, "a new --ros-edition reconfigures: {out:?}");
    assert_eq!(out.configured, ["linux"], "{out:?}");
    let again = configure_outcome(&[leaf_s, "--ros-edition", "humble"]);
    assert!(again.up_to_date, "the same flags again: {again:?}");
    cleanup(&leaf);
}

/// phase-530 W1 (I2): `--no-metadata` skips the metadata probe, so a plain
/// configure after it must run the probe, not find the stamp current.
#[test]
fn no_metadata_then_plain_configure_runs_the_probe() {
    setup();
    let leaf = scratch_leaf("no-metadata");
    let leaf_s = leaf.to_str().unwrap();
    let sidecar = leaf.join("metadata/listener.json");
    let first = configure_outcome(&[leaf_s, "--no-metadata"]);
    assert_eq!(first.configured, ["linux"], "{first:?}");
    assert!(!sidecar.exists(), "--no-metadata wrote no sidecar");
    assert!(stamp_file(&leaf, "linux").is_file(), "and recorded a stamp");
    let out = configure_outcome(&[leaf_s]);
    assert!(!out.up_to_date, "the probe never ran: {out:?}");
    assert!(sidecar.is_file(), "the plain configure ran the probe");
    cleanup(&leaf);
}

/// A copy of `multi-node-workspace` whose bringup adds an image naming a
/// board no catalog knows: stage 2 fails for it, and not as a preflight
/// refusal.
fn workspace_with_a_broken_image(tag: &str) -> PathBuf {
    let ws = scratch_copy("examples/templates/multi-node-workspace", tag);
    let sys = ws.join("src/demo_bringup/system.toml");
    let mut body = fs::read_to_string(&sys).unwrap();
    body.push_str("\n[image.broken]\nboard = \"no-such-board-anywhere\"\n");
    fs::write(&sys, body).unwrap();
    ws
}

/// Ruling R22: an image the caller did NOT name that fails stages 2-4 for
/// any reason is reported, left without a stamp, and configure still
/// succeeds -- `nros sync`'s contract (C1 succeeded, exit 0).
#[test]
fn unnamed_image_failure_does_not_fail_configure() {
    setup();
    let ws = workspace_with_a_broken_image("r22-unnamed");
    let out = configure_outcome(&[ws.to_str().unwrap()]);
    assert!(
        out.configured.iter().any(|k| k == "demo_bringup:linux"),
        "{out:?}"
    );
    assert_eq!(out.not_configured.len(), 1, "{out:?}");
    assert_eq!(out.not_configured[0].0, "demo_bringup:broken", "{out:?}");
    assert!(
        out.not_configured[0].1.contains("no-such-board-anywhere"),
        "{out:?}"
    );
    assert!(!out.not_configured[0].1.contains('\n'), "one line: {out:?}");
    assert!(
        !stamp_file(&ws, "demo_bringup:broken").exists(),
        "a failed image records no stamp"
    );
    // The same through the dispatch: exit 0.
    nros(&["configure", ws.to_str().unwrap(), "--force"])
        .expect("an unnamed image's failure does not fail configure");
    cleanup(&ws);
}

/// Ruling R22's counterpart: NAMED, the same failure is an error.
#[test]
fn named_image_failure_fails_configure() {
    setup();
    let ws = workspace_with_a_broken_image("r22-named");
    let err = nros(&[
        "configure",
        ws.to_str().unwrap(),
        "--image",
        "demo_bringup:broken",
    ])
    .expect_err("a named image's failure is an error");
    assert!(err.contains("no-such-board-anywhere"), "{err}");
    cleanup(&ws);
}

/// Issue-1305 shape (Ruling R9): a leaf's images share ONE selection package
/// and ONE `generated/` tree, so switching to another image and back must
/// reconfigure -- the first image's stamp is still on disk, and its inputs
/// have not moved, but the shared output now holds the other image.
#[test]
fn switching_leaf_images_reconfigures() {
    setup();
    let leaf = scratch_leaf("switch");
    let leaf_s = leaf.to_str().unwrap();
    let selection = leaf.join("build/nros/selection/Cargo.toml");

    let first = configure_outcome(&[leaf_s, "--image", "linux"]);
    assert_eq!(first.configured, ["linux"], "{first:?}");
    let zenoh = fs::read_to_string(&selection).unwrap();

    // Same edition (humble), another RMW.
    let other = configure_outcome(&[leaf_s, "--image", "linux_xrce"]);
    assert_eq!(other.configured, ["linux_xrce"], "{other:?}");
    let xrce = fs::read_to_string(&selection).unwrap();
    assert_ne!(zenoh, xrce, "the two images select different backends");

    let back = configure_outcome(&[leaf_s, "--image", "linux"]);
    assert!(
        !back.up_to_date,
        "the shared output holds `linux_xrce`, so `linux` is not current: {back:?}"
    );
    assert_eq!(back.configured, ["linux"], "{back:?}");
    assert_eq!(fs::read_to_string(&selection).unwrap(), zenoh);
    cleanup(&leaf);
}

/// Ruling R10: the leaf lock is seeded and pruned inside C1, before the
/// metadata probe, so nothing stage 4 does moves what the probe keyed on and
/// one sync is enough.
#[test]
fn first_configure_runs_sync_once() {
    setup();
    let leaf = scratch_leaf("once");
    let out = configure_outcome(&[leaf.to_str().unwrap()]);
    assert_eq!(out.configured, ["linux"], "{out:?}");
    assert_eq!(
        out.sync_passes, 1,
        "a fresh leaf converged in one C1: {out:?}"
    );
    cleanup(&leaf);
}

/// What `nros build` plans after a configure (stage 4, nothing spawned)
/// leaves the stamp current: planning finds the lock C1 already seeded and
/// pruned, and changes nothing a stamp reads.
#[test]
fn build_after_configure_keeps_the_stamp_current() {
    setup();
    let leaf = scratch_leaf("build-after");
    let leaf_s = leaf.to_str().unwrap();
    configure_outcome(&[leaf_s]);
    let lock = fs::read(leaf.join("Cargo.lock")).expect("configure left a lock");

    let plans = nros_cli_core::cmd::build::plan_builds(&build_args(&leaf, "linux"))
        .expect("the configured leaf plans");
    assert_eq!(plans.len(), 1);
    assert!(
        plans[0].configure.is_none(),
        "a configured leaf needs no lock prune before its build: {:?}",
        plans[0].configure
    );
    assert_eq!(fs::read(leaf.join("Cargo.lock")).unwrap(), lock);
    let again = configure_outcome(&[leaf_s]);
    assert!(
        again.up_to_date,
        "planning a build staled the stamp: {again:?}"
    );
    cleanup(&leaf);
}

/// Ruling R10: configure seeds the leaf lock from the pin set AND prunes it
/// to the leaf's graph, the pair `nros build` used to run as its first two
/// steps. Pruned means a further `cargo update --workspace` over the same
/// root and settings changes nothing.
#[test]
fn configure_seeds_and_prunes_the_leaf_lock() {
    setup();
    let leaf = scratch_leaf("lock");
    assert!(
        !leaf.join("Cargo.lock").exists(),
        "a fresh copy has no lock"
    );
    configure_outcome(&[leaf.to_str().unwrap()]);
    let lock = leaf.join("Cargo.lock");
    let seeded = fs::read(&lock).expect("configure seeded the leaf lock");

    let status = std::process::Command::new("cargo")
        .current_dir(leaf.parent().unwrap())
        .args(["update", "--workspace", "--manifest-path"])
        .arg(leaf.join("Cargo.toml"))
        .arg("--config")
        .arg(leaf.join("build/linux/nros-cargo.toml"))
        .status()
        .expect("cargo runs");
    assert!(status.success(), "cargo update over the configured leaf");
    assert_eq!(
        fs::read(&lock).unwrap(),
        seeded,
        "configure left an unpruned lock: `cargo update --workspace` still rewrote it"
    );
    cleanup(&leaf);
}

fn build_args(project: &Path, image: &str) -> nros_cli_core::cmd::build::Args {
    nros_cli_core::cmd::build::Args {
        images: vec![image.to_string()],
        workspace: Some(project.to_path_buf()),
        nano_ros_path: None,
        zephyr_workspace: None,
        board: None,
        all: false,
        dry_run: true,
        verbose: false,
        offline: false,
        selection: Default::default(),
        continue_on_error: false,
        cmake_args: Vec::new(),
        parallel_workers: None,
        build_base: None,
        check: None,
        native_args: Vec::new(),
    }
}

/// Ruling R14: the marker follows the shared output's real WRITER, not the
/// verb. `nros build <other image>` rewrites a leaf's selection package in
/// its stage 4 (`leaf_settings::write_selection`) without running configure,
/// and the configured image must read stale afterwards.
#[test]
fn selection_written_outside_configure_moves_the_marker() {
    setup();
    let leaf = scratch_leaf("marker-writer");
    let leaf_s = leaf.to_str().unwrap();
    configure_outcome(&[leaf_s, "--image", "linux"]);

    nros_cli_core::cmd::leaf_settings::write_selection(&leaf, &repo_root(), Some("linux_xrce"))
        .expect("the xrce image's selection writes");

    let out = configure_outcome(&[leaf_s, "--image", "linux"]);
    assert!(
        !out.up_to_date,
        "the selection now holds `linux_xrce`, written outside configure: {out:?}"
    );
    assert_eq!(out.configured, ["linux"], "{out:?}");
    cleanup(&leaf);
}

/// Ruling R15 — a host lacking one image's toolchain must not reconfigure on
/// every bare `nros build`: the skipped image's SKIP stamp is current for a
/// configure that does not name it.
#[test]
fn skipped_image_does_not_reconfigure_every_build() {
    setup();
    let ws = workspace_with_an_unprovisioned_image("skip-noop");
    let first = configure_outcome(&[ws.to_str().unwrap()]);
    assert_eq!(first.skipped.len(), 1, "{first:?}");
    let second = configure_outcome(&[ws.to_str().unwrap()]);
    assert!(
        second.up_to_date,
        "nothing changed, so nothing reconfigures: {second:?}"
    );
    assert_eq!(second.sync_passes, 0, "{second:?}");
    cleanup(&ws);
}

/// Ruling R15 — a bringup-less workspace declares no image, so it records one
/// project-level stamp, and an unchanged second configure is a no-op.
#[test]
fn imageless_workspace_second_configure_is_a_no_op() {
    setup();
    let ws = scratch_copy("examples/templates/workspace-shadowing", "imageless");
    let first = configure_outcome(&[ws.to_str().unwrap()]);
    assert!(!first.up_to_date, "{first:?}");
    assert_eq!(first.sync_passes, 1, "{first:?}");
    let second = configure_outcome(&[ws.to_str().unwrap()]);
    assert!(second.up_to_date, "{second:?}");
    assert_eq!(second.sync_passes, 0, "{second:?}");
    cleanup(&ws);
}
