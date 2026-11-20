//! Issue 2645: `nros-toolchain.toml` names a commit (`git` + `rev`) or a
//! checkout (`path`), beside the release `version` it always took.
//!
//! Hermetic: the "remote" is a local git repository whose
//! `scripts/bootstrap.sh` is a stub that writes the CLI a real one would
//! build, and `cargo` is a stub on a test-only `PATH`. What is measured is
//! the launcher's half: the schema, the path it constructs, the atomic store
//! install, and that the release path is untouched.

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::Command,
};

use nros_launcher::{
    dispatch, git_pin,
    launch::{self, Context, Plan, Source},
    pin::{self, PinSource},
    session::Session,
};

const REV: &str = "09219edc75c9b9a788590c6ac586a7ba45185eab";

fn write(path: &Path, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

fn executable(path: &Path, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    nros_launcher::test_support::write_executable_stub(path, body);
}

fn pin_file(dir: &Path, body: &str) -> PathBuf {
    let p = dir.join(pin::FILE_NAME);
    write(&p, &format!("[toolchain]\n{body}"));
    p
}

fn ctx(cwd: &Path, store: &Path) -> Context {
    Context {
        exe: store.join("bin").join("nros"),
        cwd: cwd.to_path_buf(),
        store: store.to_path_buf(),
        dispatched: None,
        session: Session::interactive(),
    }
}

/// A directory that `checkout::is_monorepo_root` accepts.
fn fake_checkout(dir: &Path) {
    write(
        &dir.join(nros_launcher::checkout::MONOREPO_MARKER),
        "[package]\n",
    );
}

// ---------------------------------------------------------------------------
// The schema
// ---------------------------------------------------------------------------

#[test]
fn a_git_pin_names_its_store_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let p = pin_file(
        tmp.path(),
        &format!("git = \"https://example.invalid/nano-ros\"\nrev = \"{REV}\"\n"),
    );
    let got = pin::load(&p).unwrap();
    assert_eq!(got.version, format!("git-{REV}"));
    assert_eq!(
        got.source,
        PinSource::Git {
            url: "https://example.invalid/nano-ros".into(),
            rev: REV.into()
        }
    );
    assert_eq!(
        got.candidate_bins(Path::new("/s"))[0],
        Path::new("/s/toolchains")
            .join(format!("git-{REV}"))
            .join("bin/nros")
    );
}

#[test]
fn a_rev_that_is_not_a_full_sha_is_refused_with_the_reason() {
    let tmp = tempfile::tempdir().unwrap();
    for (rev, says) in [
        ("main", "branch or tag"),
        ("nros-0.6.0-nros11", "branch or tag"),
        ("09219edc75", "10-character sha"),
    ] {
        let p = pin_file(
            tmp.path(),
            &format!("git = \"https://example.invalid/n\"\nrev = \"{rev}\"\n"),
        );
        let e = format!("{:#}", pin::load(&p).unwrap_err());
        assert!(e.contains(says), "{rev}: {e}");
        assert!(e.contains("full 40-hex commit"), "{rev}: {e}");
    }
}

#[test]
fn the_forms_are_exclusive_and_rev_needs_git() {
    let tmp = tempfile::tempdir().unwrap();
    let cases = [
        (
            format!("version = \"0.6.0-nros11\"\ngit = \"u\"\nrev = \"{REV}\"\n"),
            "more than one",
        ),
        (format!("rev = \"{REV}\"\n"), "`rev` without `git`"),
        ("git = \"u\"\n".to_string(), "`git` without `rev`"),
        ("revision = \"x\"\n".to_string(), "names no toolchain"),
        (String::new(), "names no toolchain"),
    ];
    for (body, says) in cases {
        let p = pin_file(tmp.path(), &body);
        let e = format!("{:#}", pin::load(&p).unwrap_err());
        assert!(e.contains(says), "{body:?}: {e}");
    }
}

#[test]
fn a_path_pin_is_relative_to_its_file_and_must_be_a_checkout() {
    let tmp = tempfile::tempdir().unwrap();
    let proj = tmp.path().join("proj");
    let checkout = tmp.path().join("proj/modules/nros");
    fake_checkout(&checkout);
    let p = pin_file(&proj, "path = \"modules/nros\"\n");
    let got = pin::load(&p).unwrap();
    let abs = std::fs::canonicalize(&checkout).unwrap();
    assert_eq!(got.source, PinSource::Path(abs.clone()));
    assert_eq!(
        got.candidate_bins(Path::new("/s")),
        vec![pin::checkout_cli_bin(&abs)]
    );

    let p = pin_file(&proj, "path = \"modules\"\n");
    let e = format!("{:#}", pin::load(&p).unwrap_err());
    assert!(e.contains("not a nano-ros checkout"), "{e}");
}

#[test]
fn a_release_pin_reads_as_it_always_did() {
    let tmp = tempfile::tempdir().unwrap();
    let p = pin::write(tmp.path(), "0.6.0-nros11").unwrap();
    let got = pin::load(&p).unwrap();
    assert_eq!(got, pin::Pin::release(p, "0.6.0-nros11"));
}

// ---------------------------------------------------------------------------
// Resolution
// ---------------------------------------------------------------------------

#[test]
fn a_git_pin_resolves_to_its_store_directory_or_is_missing() {
    let tmp = tempfile::tempdir().unwrap();
    let store = tmp.path().join("store");
    let proj = tmp.path().join("proj");
    pin_file(&proj, &format!("git = \"u\"\nrev = \"{REV}\"\n"));

    let plan = launch::resolve(&ctx(&proj, &store)).unwrap();
    let Plan::Missing {
        source, looked_in, ..
    } = &plan
    else {
        panic!("expected Missing, got {plan:?}");
    };
    assert!(matches!(source, PinSource::Git { .. }));
    assert_eq!(
        looked_in[0],
        git_pin::store_dir(&store, REV).join("bin/nros")
    );

    executable(&looked_in[0], "#!/bin/sh\n");
    let plan = launch::resolve(&ctx(&proj, &store)).unwrap();
    assert!(
        matches!(&plan, Plan::Exec { bin, source: Source::Pin(_), .. } if *bin == looked_in[0]),
        "{plan:?}"
    );
}

#[test]
fn a_git_toolchain_is_never_the_unpinned_default() {
    let tmp = tempfile::tempdir().unwrap();
    let store = tmp.path().join("store");
    executable(
        &store.join("toolchains/0.6.0-nros11/bin/nros"),
        "#!/bin/sh\n",
    );
    executable(
        &git_pin::store_dir(&store, REV).join("bin/nros"),
        "#!/bin/sh\n",
    );
    assert_eq!(launch::installed_versions(&store), vec!["0.6.0-nros11"]);
}

#[test]
fn a_path_pin_resolves_to_the_checkouts_own_build() {
    let tmp = tempfile::tempdir().unwrap();
    let store = tmp.path().join("store");
    let proj = tmp.path().join("proj");
    let checkout = tmp.path().join("nros");
    fake_checkout(&checkout);
    pin_file(&proj, &format!("path = \"{}\"\n", checkout.display()));

    // Not built yet: the remedy is the checkout's own front door, and nothing
    // is fetched.
    let plan = launch::resolve(&ctx(&proj, &store)).unwrap();
    let Plan::Missing {
        version,
        pin_path,
        looked_in,
        source,
    } = &plan
    else {
        panic!("expected Missing, got {plan:?}");
    };
    let e = dispatch::fetch(&store, &store, version, pin_path, looked_in, source).unwrap_err();
    assert!(format!("{e:#}").contains("scripts/bootstrap.sh"), "{e:#}");

    let built = pin::checkout_cli_bin(&checkout);
    executable(&built, "#!/bin/sh\n");
    let plan = launch::resolve(&ctx(&proj, &store)).unwrap();
    assert!(
        matches!(&plan, Plan::Exec { bin, .. } if std::fs::canonicalize(bin).unwrap() == std::fs::canonicalize(&built).unwrap()),
        "{plan:?}"
    );
}

// ---------------------------------------------------------------------------
// The install
// ---------------------------------------------------------------------------

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.invalid")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.invalid")
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

/// A repository whose one commit carries a `bootstrap.sh` stub. `ok` decides
/// whether the stub builds the CLI or fails.
fn upstream(dir: &Path, ok: bool) -> String {
    std::fs::create_dir_all(dir).unwrap();
    git(dir, &["init", "-q"]);
    // The CLI a real bootstrap.sh builds, relative to the tree's root.
    let cli = pin::checkout_cli_bin(Path::new("."));
    let cli = cli.display();
    let body = if ok {
        format!(
            "#!/usr/bin/env bash\nset -e\n\
             [ \"$1\" = --no-prompt ] || exit 9\n\
             command -v cargo >/dev/null || exit 8\n\
             ! command -v nros >/dev/null || exit 7\n\
             mkdir -p \"$(dirname {cli})\"\n\
             printf '#!/bin/sh\\n[ \"$1\" = source-stamp ] && echo fresh\\nexit 0\\n' > {cli}\n\
             chmod +x {cli}\n"
        )
    } else {
        "#!/usr/bin/env bash\nexit 1\n".to_string()
    };
    executable(&dir.join("scripts/bootstrap.sh"), &body);
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", "c"]);
    git(dir, &["rev-parse", "HEAD"])
}

/// `PATH` with a stub `cargo`, the real `git` and `bash`, and a directory that
/// holds an `nros` (a launcher), which the install must take out of the
/// build's `PATH`.
fn build_path(tmp: &Path) -> OsString {
    let tools = tmp.join("tools");
    executable(&tools.join("cargo"), "#!/bin/sh\nexit 0\n");
    let launcher = tmp.join("launcher-bin");
    executable(&launcher.join("nros"), "#!/bin/sh\nexit 0\n");
    let mut dirs = vec![launcher, tools];
    dirs.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    std::env::join_paths(dirs).unwrap()
}

fn toolchain_names(store: &Path) -> std::collections::BTreeSet<String> {
    let dir = store.join("toolchains");
    std::fs::read_dir(dir)
        .map(|d| {
            d.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn a_git_pin_installs_atomically_and_once() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("upstream");
    let rev = upstream(&repo, true);
    let store = tmp.path().join("store");
    let path = build_path(tmp.path());
    let url = repo.display().to_string();
    let pin_path = tmp.path().join(pin::FILE_NAME);

    let bin = git_pin::install_with_path(&store, &url, &rev, &pin_path, &path).unwrap();
    assert_eq!(bin, git_pin::store_dir(&store, &rev).join("bin/nros"));
    assert!(bin.is_file(), "bin/nros resolves to the built CLI");
    let dir = git_pin::store_dir(&store, &rev);
    assert_eq!(git(&dir, &["rev-parse", "HEAD"]), rev);
    // Only the finished name: no partial left behind.
    assert_eq!(
        toolchain_names(&store),
        std::iter::once(format!("git-{rev}")).collect()
    );

    // The second ask is the lookup, not a rebuild: a broken upstream now
    // cannot matter.
    std::fs::remove_dir_all(&repo).unwrap();
    assert_eq!(
        git_pin::install_with_path(&store, &url, &rev, &pin_path, &path).unwrap(),
        bin
    );
}

#[test]
fn a_failed_build_leaves_nothing_in_the_store() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("upstream");
    let rev = upstream(&repo, false);
    let store = tmp.path().join("store");
    let path = build_path(tmp.path());
    let e = git_pin::install_with_path(
        &store,
        &repo.display().to_string(),
        &rev,
        &tmp.path().join(pin::FILE_NAME),
        &path,
    )
    .unwrap_err();
    assert!(format!("{e:#}").contains("bootstrap.sh"), "{e:#}");
    assert!(
        toolchain_names(&store).is_empty(),
        "{:?}",
        toolchain_names(&store)
    );
}

#[test]
fn a_rev_the_repository_lacks_is_an_error_not_a_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("upstream");
    upstream(&repo, true);
    let store = tmp.path().join("store");
    let e = git_pin::install_with_path(
        &store,
        &repo.display().to_string(),
        REV,
        &tmp.path().join(pin::FILE_NAME),
        &build_path(tmp.path()),
    )
    .unwrap_err();
    assert!(format!("{e:#}").contains("git fetch"), "{e:#}");
    assert!(toolchain_names(&store).is_empty());
}

#[test]
fn no_cargo_is_refused_and_rustup_is_not_installed() {
    let tmp = tempfile::tempdir().unwrap();
    let store = tmp.path().join("store");
    // A PATH with git only, through a directory of its own.
    let only_git = tmp.path().join("only-git");
    let real_git = String::from_utf8(
        Command::new("sh")
            .args(["-c", "command -v git"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    std::fs::create_dir_all(&only_git).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(real_git.trim(), only_git.join("git")).unwrap();
    let e = git_pin::install_with_path(
        &store,
        "u",
        REV,
        &tmp.path().join(pin::FILE_NAME),
        only_git.as_os_str(),
    )
    .unwrap_err();
    let e = format!("{e:#}");
    assert!(
        e.contains("`cargo` is not on PATH") && e.contains("rustup"),
        "{e}"
    );
    assert!(!store.join("toolchains").exists() || toolchain_names(&store).is_empty());
}

#[test]
fn a_dead_installers_partial_is_reaped_and_a_live_one_kept() {
    let tmp = tempfile::tempdir().unwrap();
    let store = tmp.path();
    let dead = store.join(format!("toolchains/.git-{REV}.partial-4294967294"));
    let live = store.join(format!(
        "toolchains/.git-{REV}.partial-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dead).unwrap();
    std::fs::create_dir_all(&live).unwrap();
    let removed = git_pin::reap_partials(store);
    if cfg!(target_os = "linux") {
        assert_eq!(removed, vec![dead.clone()]);
        assert!(!dead.exists());
    }
    assert!(live.exists());
}
