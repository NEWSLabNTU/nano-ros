//! The store's reclaim verbs — `nros store list|gc` and `nros toolchain
//! uninstall` (phase-440 W6, RFC-0095 D11).
//!
//! Every test here is about a REFUSAL, because that is what this feature is:
//! the reclaim path is trivial (`remove_dir_all`) and the whole design is the
//! set of conditions under which it does not run. The two that RFC-0095 D11
//! states in prose are asserted directly —
//!
//! * `--dry-run` is the default, and
//! * nothing a pin names is ever removed
//!
//! — plus the two that are one step behind them and would otherwise be
//! "the code looks right": that an empty pin set refuses instead of permitting,
//! and that the scan does not destroy the timestamps it reads.
//!
//! No test here touches `$NROS_HOME`, `$NROS_STORE` or the process working
//! directory. The store root is a `--root` argument and the pin search
//! directory is a parameter, so these run in parallel with everything else and
//! observe nothing global (issue 1101's hazard).

use std::{
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use nros_cli_core::{
    cmd::{store as store_cmd, toolchain as toolchain_cmd},
    orchestration::{
        sdk_store::{Provenance, ProvenanceKind},
        store,
    },
};

/// A store entry that looks exactly like one `sdk_store::execute` wrote: a
/// payload file plus the `.nros-provenance` marker that makes it `installed`.
fn install(root: &Path, rel: &str, version: &str, payload: &[u8]) -> PathBuf {
    let dir = root.join(rel);
    std::fs::create_dir_all(dir.join("bin")).unwrap();
    std::fs::write(dir.join("bin").join("tool"), payload).unwrap();
    Provenance {
        kind: ProvenanceKind::Prebuilt,
        version: version.to_string(),
        sha256: None,
    }
    .write(&dir)
    .unwrap();
    dir
}

/// A directory in the store with no provenance marker — a partial unpack, a
/// hand-made directory, anything this tool did not write.
fn unattributable(root: &Path, rel: &str) -> PathBuf {
    let dir = root.join(rel);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("stuff"), b"whatever").unwrap();
    dir
}

fn pin_file(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, body).unwrap();
    path
}

fn gc_args(root: &Path, older_than: &str) -> store_cmd::GcArgs {
    store_cmd::GcArgs {
        older_than: older_than.to_string(),
        delete: false,
        dry_run: false,
        pin_file: Vec::new(),
        ignore_pins: false,
        root: Some(root.to_path_buf()),
    }
}

fn uninstall_args(root: &Path, version: &str) -> toolchain_cmd::UninstallArgs {
    toolchain_cmd::UninstallArgs {
        version: version.to_string(),
        pin_file: Vec::new(),
        ignore_pins: false,
        dry_run: false,
        root: Some(root.to_path_buf()),
    }
}

/// RFC-0095 D11's first requirement, asserted on the filesystem rather than on
/// the flag: a `gc` that names no `--delete` must leave every byte in place,
/// even for an entry it has just reported as collectable.
///
/// `--older-than 0s` makes every unpinned install a candidate, so the only
/// thing standing between this entry and deletion is the default.
#[test]
fn gc_is_a_dry_run_unless_delete_is_named() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let entry = install(root, "sdk/qemu/11.0.0", "11.0.0", b"binary");
    let pins = pin_file(
        tmp.path(),
        "nros-toolchain.toml",
        "version = \"something-else\"\n",
    );

    let mut args = gc_args(root, "0s");
    args.pin_file = vec![pins.clone()];
    store_cmd::gc(args, None).unwrap();
    assert!(
        entry.is_dir(),
        "the default must remove nothing — {} is gone",
        entry.display()
    );

    let mut args = gc_args(root, "0s");
    args.pin_file = vec![pins];
    args.delete = true;
    store_cmd::gc(args, None).unwrap();
    assert!(
        !entry.exists(),
        "--delete must actually remove: {} survived",
        entry.display()
    );
}

/// RFC-0095 D11's second requirement. The entry is as old and as collectable as
/// the age filter can make it (`--older-than 0s`), `--delete` is named, and it
/// still survives — because a pin file names its version.
///
/// Deliberately run through the COMMAND, not through `plan_gc`: a plan that
/// classifies correctly and a verb that deletes the right rows are two claims,
/// and only the second one is what a user gets.
#[test]
fn gc_never_removes_an_entry_a_pin_names() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let pinned = install(root, "sdk/qemu/11.0.0-nros2", "11.0.0-nros2", b"pinned");
    let unpinned = install(root, "sdk/qemu/9.0.0-nros1", "9.0.0-nros1", b"stale");

    // The structured pin source this crate already owns: an SDK index naming a
    // tool and the version of it that is in use.
    let index = pin_file(
        tmp.path(),
        "nros-sdk-index.toml",
        "[tool.qemu]\nversion = \"11.0.0-nros2\"\n",
    );

    let mut args = gc_args(root, "0s");
    args.pin_file = vec![index];
    args.delete = true;
    store_cmd::gc(args, None).unwrap();

    assert!(
        pinned.is_dir(),
        "a pinned entry was removed: {}",
        pinned.display()
    );
    assert!(
        !unpinned.exists(),
        "the unpinned entry should have been collected: {}",
        unpinned.display()
    );
}

/// A pin protects regardless of age — asserted on the PLAN, where a synthetic
/// `now` can push the entry a century past any threshold.
///
/// This is the ordering the implementation depends on: if `plan_gc` filtered by
/// age first, the pin test would never run for exactly the entries that matter,
/// and every test above would still pass because their entries are new.
#[test]
fn a_pin_protects_an_entry_however_old_it_is() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    install(root, "sdk/qemu/11.0.0-nros2", "11.0.0-nros2", b"pinned");
    let index = pin_file(
        tmp.path(),
        "nros-sdk-index.toml",
        "[tool.qemu]\nversion = \"11.0.0-nros2\"\n",
    );
    let sources = store::collect_pins(&[index], None).unwrap();

    let a_century = Duration::from_secs(100 * 365 * 24 * 60 * 60);
    let plan = store::plan_gc(
        store::scan(root),
        Duration::from_secs(1),
        &sources,
        SystemTime::now() + a_century,
    );

    assert!(
        plan.remove.is_empty(),
        "an entry a pin names was scheduled for removal: {:?}",
        plan.remove
            .iter()
            .map(|e| e.display_id())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        plan.pinned().count(),
        1,
        "the pin should be REPORTED, not silent"
    );
}

/// "No pin names this" and "no pin file exists to name anything" are different
/// answers, and only the first one may authorise a delete.
///
/// Without this refusal, `nros store gc --older-than 90d --delete` run from a
/// directory with no project above it would collect another project's
/// toolchains and report success — the store is shared while pins are
/// per-project.
#[test]
fn gc_refuses_to_delete_when_no_pin_file_could_be_consulted() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let entry = install(root, "sdk/qemu/11.0.0", "11.0.0", b"binary");

    let mut args = gc_args(root, "0s");
    args.delete = true;
    let err = store_cmd::gc(args, None).unwrap_err().to_string();

    assert!(
        err.contains("refusing to delete"),
        "expected a refusal naming its reason, got: {err}"
    );
    assert!(
        err.contains("--ignore-pins"),
        "a refusal must name the way past it, got: {err}"
    );
    assert!(entry.is_dir(), "the refusal must not have deleted anything");

    // And the override is a real door, not decoration.
    let mut args = gc_args(root, "0s");
    args.delete = true;
    args.ignore_pins = true;
    store_cmd::gc(args, None).unwrap();
    assert!(!entry.exists(), "--ignore-pins should have collected it");
}

/// `gc` removes only what it can attribute to an install. A directory with no
/// provenance marker was written by something this tool does not model — the
/// legacy flat prefix `nros sdk-path` still resolves through (issue 0628), a
/// source build tree, a partial unpack — and removing one can un-provision a
/// working host.
#[test]
fn gc_leaves_entries_it_cannot_attribute_to_an_install() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let partial = unattributable(root, "sdk/corrosion/0.6.1-nros1");
    let source_tree = unattributable(root, "sdk/corrosion/0.6.src");
    let cache = unattributable(root, "fetch/some-tarball-dir");

    let mut args = gc_args(root, "0s");
    args.delete = true;
    args.ignore_pins = true;
    store_cmd::gc(args, None).unwrap();

    for path in [&partial, &source_tree, &cache] {
        assert!(
            path.is_dir(),
            "an entry with no provenance marker was removed: {}",
            path.display()
        );
    }
}

/// `list`'s contract: a size and a last-used time per entry, and a state that
/// says whether the number can be acted on.
#[test]
fn the_listing_reports_a_size_and_a_last_use_for_every_entry() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    install(root, "sdk/qemu/11.0.0", "11.0.0", &vec![7u8; 4096]);
    unattributable(root, "sdk/corrosion/0.6.src");

    let entries = store::scan(root);
    assert_eq!(entries.len(), 2, "scan found {entries:#?}");

    let qemu = entries
        .iter()
        .find(|e| e.display_id() == "sdk/qemu/11.0.0")
        .expect("the installed entry is missing from the listing");
    assert!(
        qemu.size_bytes >= 4096,
        "size must count the payload, got {}",
        qemu.size_bytes
    );
    assert_eq!(qemu.state, store::EntryState::Installed);
    assert!(
        qemu.last_used <= SystemTime::now(),
        "last-used must be a real timestamp, not a placeholder in the future"
    );

    let src = entries
        .iter()
        .find(|e| e.display_id() == "sdk/corrosion/0.6.src")
        .expect("the source tree is missing from the listing");
    assert_eq!(
        src.state,
        store::EntryState::SourceTree,
        "a `<version>.src` tree must be named as one — it is the biggest \
         reclaimable thing in a real store and `unmanaged` tells a human nothing"
    );
}

/// The scan must not consume the evidence it reports.
///
/// It did, on its first run: deciding an entry's state opens
/// `.nros-provenance`, which updates that file's atime, so every installed
/// entry read "last used 1s ago" and `gc --older-than 90d` would have found
/// nothing to collect on any host, forever. Walking the tree updates every
/// DIRECTORY's atime for the same reason. Two scans in a row must agree.
#[test]
fn scanning_the_store_does_not_change_what_it_measures() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    install(root, "sdk/qemu/11.0.0", "11.0.0", b"binary");

    // The sleep is what gives this test its teeth, and it was missing at first:
    // the kernel stamps timestamps from a COARSE clock (one tick, single-digit
    // ms), so a file written and read inside one tick gets an `atime` byte-equal
    // to its `mtime` — and the read the bug is about becomes invisible. Without
    // the wait, this test passed with the exclusion removed. One tick is enough;
    // 50 ms is several.
    std::thread::sleep(Duration::from_millis(50));

    let first = store::scan(root);
    let second = store::scan(root);
    assert_eq!(first.len(), 1);
    assert_eq!(
        first[0].last_used, second[0].last_used,
        "the second scan reported a different last-used time — the scan is \
         reading files whose atime it then measures"
    );
    assert_eq!(
        first[0].evidence, second[0].evidence,
        "the second scan reported different usage evidence"
    );
    assert_eq!(
        first[0].evidence,
        store::UseEvidence::InstallOnly,
        "nothing has read this entry, so its timestamp is the install"
    );
}

/// Pin discovery walks up from the directory it is given, the way cargo finds a
/// workspace root — a user runs a store verb from wherever they are inside
/// their project.
#[test]
fn pin_discovery_walks_up_from_the_directory_it_is_given() {
    let tmp = tempfile::tempdir().unwrap();
    pin_file(
        tmp.path(),
        "nros-toolchain.toml",
        "[toolchain]\nversion = \"0.6.2\"\n",
    );
    let deep = tmp.path().join("src").join("nested").join("deeper");
    std::fs::create_dir_all(&deep).unwrap();

    let sources = store::collect_pins(&[], Some(&deep)).unwrap();
    assert_eq!(sources.len(), 1, "found {sources:#?}");
    assert!(
        sources[0]
            .rules
            .contains(&store::PinRule::AnyVersion("0.6.2".into())),
        "the pin's version did not survive the read: {:?}",
        sources[0].rules
    );
}

/// `toolchain uninstall` refuses while a known pin names the version — the
/// requirement RFC-0095 D11 states for this verb.
///
/// The pin is `nros-toolchain.toml`, phase-440 W7's file. Its SCHEMA is W7's to
/// choose, so the reader takes any string value in it as a version: a delete
/// guard that a later phase's key name can silently disarm is not a guard.
#[test]
fn toolchain_uninstall_refuses_while_a_pin_names_the_version() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let entry = install(root, "toolchains/0.6.2", "0.6.2", b"toolchain");
    let pin = pin_file(
        tmp.path(),
        "nros-toolchain.toml",
        "[toolchain]\nversion = \"0.6.2\"\n",
    );

    let mut args = uninstall_args(root, "0.6.2");
    args.pin_file = vec![pin];
    let err = toolchain_cmd::uninstall(args, None)
        .unwrap_err()
        .to_string();

    assert!(
        err.contains("pinned by"),
        "the refusal must name the pin file, got: {err}"
    );
    assert!(entry.is_dir(), "a pinned toolchain was removed");
}

/// The case today's tree actually reaches: no `nros-toolchain.toml` exists
/// anywhere yet (W7 introduces it), so nothing can establish that no project
/// pins this version — and unestablished is not the same as false.
#[test]
fn toolchain_uninstall_refuses_when_no_pin_file_could_be_consulted() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let entry = install(root, "toolchains/0.6.2", "0.6.2", b"toolchain");

    let err = toolchain_cmd::uninstall(uninstall_args(root, "0.6.2"), None)
        .unwrap_err()
        .to_string();

    assert!(
        err.contains("cannot verify"),
        "the refusal must say what it could not establish, got: {err}"
    );
    assert!(
        entry.is_dir(),
        "nothing may be removed on an unverified pin set"
    );
}

/// With a pin set that WAS consulted and does not name the version, the verb
/// does its job — otherwise the refusals above would just be a broken command.
#[test]
fn toolchain_uninstall_removes_a_version_no_consulted_pin_names() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let keep = install(root, "toolchains/0.7.0", "0.7.0", b"current");
    let drop = install(root, "toolchains/0.6.2", "0.6.2", b"old");
    let pin = pin_file(
        tmp.path(),
        "nros-toolchain.toml",
        "[toolchain]\nversion = \"0.7.0\"\n",
    );

    let mut args = uninstall_args(root, "0.6.2");
    args.pin_file = vec![pin];
    toolchain_cmd::uninstall(args, None).unwrap();

    assert!(!drop.exists(), "the unpinned toolchain should be gone");
    assert!(keep.is_dir(), "the pinned toolchain must be untouched");
}

/// An absent toolchain is reported, not an error and not a delete: there is
/// nothing to remove, so the pin question never arises. This is the branch
/// every invocation reaches today, since `toolchains/` is W7's directory.
#[test]
fn toolchain_uninstall_reports_an_absent_version_rather_than_failing() {
    let tmp = tempfile::tempdir().unwrap();
    toolchain_cmd::uninstall(uninstall_args(tmp.path(), "0.6.2"), None)
        .expect("an absent toolchain is not an error — there is nothing to refuse");
}
