//! A scaffolded board resolves, and its acceptance is the gates, not the files.
//!
//! phase-375 W2/W9. The wave exists because `s32z270` landed RED on five gates
//! and `freertos-posix` on two — each gate correct, the cost being that they
//! were discovered serially, on main, by whoever noticed. So the assertions
//! below are about what the gates and the catalog think of the output, not
//! about which files appeared.

use std::process::Command;

fn nros() -> PathBuf {
    // The binary cargo builds for THIS test run. This test lived in
    // `nros-cli-core` and found `nros` beside its own binary, which
    // `cargo test -p nros-cli-core` never rebuilds, so a per-crate run tested
    // whatever CLI an earlier build left there (`unknown field ...` against a
    // board descriptor newer than that binary). It moved here for
    // `CARGO_BIN_EXE_nros`.
    PathBuf::from(env!("CARGO_BIN_EXE_nros"))
}

use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("repo root")
        .to_path_buf()
}

fn scaffold(tmp: &std::path::Path, args: &[&str]) -> String {
    let out = Command::new(nros())
        .arg("board")
        .arg("new")
        .args(args)
        .arg("--out-of-tree")
        .arg(tmp)
        .arg("--workspace")
        .arg(repo_root())
        .output()
        .expect("run nros board new");
    assert!(
        out.status.success(),
        "scaffold failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// The whole claim of RFC-0064 R5 D1, end to end: a user's board is our board's
/// shape, and the only difference is which root found it.
#[test]
fn a_scaffolded_out_of_tree_board_resolves_by_name() {
    let tmp = tempfile::tempdir().expect("tempdir");
    scaffold(tmp.path(), &["vendorboard", "--platform", "freertos"]);

    let out = Command::new(nros())
        .args(["board", "info", "vendorboard", "--workspace"])
        .arg(repo_root())
        .env("NROS_EXTRA_BOARD_PATH", tmp.path())
        .output()
        .expect("run nros board info");
    assert!(
        out.status.success(),
        "a scaffolded board must resolve with no in-tree edit beyond the search \
         root: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// The announcement is not optional, so the scaffold must write it (RFC-0064
/// R5 D5). Without this the catalog refuses the board — which is the behaviour
/// we want, and exactly why the scaffold has to get it right.
#[test]
fn the_scaffold_writes_the_announcement_the_catalog_requires() {
    let tmp = tempfile::tempdir().expect("tempdir");
    scaffold(tmp.path(), &["announcedboard", "--platform", "freertos"]);
    let dir = tmp.path().join("nros-board-announcedboard");

    let xml = std::fs::read_to_string(dir.join("package.xml")).expect("package.xml written");
    assert!(
        xml.contains(r#"<nano_ros_provides kind="board" name="announcedboard"/>"#),
        "the announcement must name the board: {xml}"
    );

    // And the negative: remove it, and the board stops resolving. The scaffold
    // is only load-bearing if its absence is fatal.
    std::fs::remove_file(dir.join("package.xml")).unwrap();
    let out = Command::new(nros())
        .args(["board", "info", "announcedboard", "--workspace"])
        .arg(repo_root())
        .env("NROS_EXTRA_BOARD_PATH", tmp.path())
        .output()
        .expect("run nros board info");
    assert!(
        !out.status.success(),
        "a board with no package.xml must NOT resolve — if it does, the \
         announcement is decoration"
    );
}

/// A Zephyr board's derived filenames are the ones the projection looks for.
///
/// The scaffold PRINTS those paths, and the projection DERIVES them. If the two
/// rules ever disagree, a board author creates `boards/<x>.conf` at the printed
/// path and the build silently never reads it.
#[test]
fn the_paths_the_scaffold_prints_are_the_paths_the_projection_derives() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let stdout = scaffold(
        tmp.path(),
        &[
            "zephyrboard",
            "--platform",
            "zephyr",
            "--west-board",
            "plank_r5/plank/smp",
        ],
    );
    let dir = tmp.path().join("nros-board-zephyrboard");
    let conf = dir.join("boards").join("plank_r5_plank_smp.conf");
    assert!(
        stdout.contains(&conf.display().to_string()),
        "the scaffold must print the derived conf path; got:\n{stdout}"
    );

    // Create it at the printed path, and the projection must pick it up.
    std::fs::create_dir_all(conf.parent().unwrap()).unwrap();
    std::fs::write(&conf, "# board conf\n").unwrap();
    let out_file = tmp.path().join("vars.cmake");
    let out = Command::new(nros())
        .args(["board", "cmake-vars", "zephyrboard", "--workspace"])
        .arg(repo_root())
        .arg("--out")
        .arg(&out_file)
        .env("NROS_EXTRA_BOARD_PATH", tmp.path())
        .output()
        .expect("run nros dev board cmake-vars");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let vars = std::fs::read_to_string(&out_file).unwrap();
    assert!(
        vars.contains(&format!(
            "set(NROS_BOARD_BOARD_CONF \"{}\")",
            conf.display()
        )),
        "a conf created at the printed path must reach cmake; got:\n{vars}"
    );
}

/// A key that is legal as a directory but not as a cargo feature or a cmake
/// value fails HERE, not three tools later.
#[test]
fn an_illegal_board_key_is_refused_up_front() {
    let tmp = tempfile::tempdir().expect("tempdir");
    for bad in ["Vendor_Board", "vendor board", "vendor/board"] {
        let out = Command::new(nros())
            .args([
                "board",
                "new",
                bad,
                "--platform",
                "freertos",
                "--out-of-tree",
            ])
            .arg(tmp.path())
            .arg("--workspace")
            .arg(repo_root())
            .output()
            .expect("run nros board new");
        assert!(!out.status.success(), "`{bad}` must be refused");
    }
}

/// `--platform zephyr` without `--west-board` cannot produce a usable board:
/// the conf/overlay filenames and the Rust-support Kconfig symbol all derive
/// from it.
#[test]
fn a_zephyr_board_without_its_board_id_is_refused() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let out = Command::new(nros())
        .args([
            "board",
            "new",
            "noid",
            "--platform",
            "zephyr",
            "--out-of-tree",
        ])
        .arg(tmp.path())
        .arg("--workspace")
        .arg(repo_root())
        .output()
        .expect("run nros board new");
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("--west-board"),
        "the refusal must name the fix: {err}"
    );
}

/// phase-528 W2 — a scaffolded host board states `[board.run] host = true`,
/// and the descriptor loads under the strict schema with it.
#[test]
fn a_scaffolded_host_board_states_how_it_runs() {
    let tmp = tempfile::tempdir().expect("tempdir");
    scaffold(tmp.path(), &["hostboard", "--platform", "posix"]);
    let text = std::fs::read_to_string(tmp.path().join("nros-board-hostboard/nros-board.toml"))
        .expect("descriptor written");
    assert!(
        text.contains("[board.run]\nhost = true\n") && !text.contains("runner ="),
        "{text}"
    );
    let out = Command::new(nros())
        .args(["board", "info", "hostboard", "--workspace"])
        .arg(repo_root())
        .env("NROS_EXTRA_BOARD_PATH", tmp.path())
        .output()
        .expect("run nros board info");
    assert!(
        out.status.success(),
        "the scaffolded descriptor must load: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A temp directory with no nano-ros checkout above it: the installed-track
/// user's cwd. Asserted, not assumed, so a TMPDIR inside a checkout fails this
/// precondition loudly instead of passing the test for the wrong reason.
fn outside_every_checkout() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("tempdir");
    assert!(
        tmp.path()
            .ancestors()
            .all(|a| !a.join("packages").join("boards").is_dir()),
        "the temp dir must sit outside every checkout: {}",
        tmp.path().display()
    );
    tmp
}

/// Issue 2291 -- the out-of-tree road writes nothing into a checkout, so it
/// must not need one. Run from a directory no checkout encloses, with no
/// `--workspace` and no `$NROS_WORKSPACE_ROOT`.
#[test]
fn out_of_tree_needs_no_checkout() {
    let tmp = outside_every_checkout();
    let boards = tmp.path().join("boards");
    let out = Command::new(nros())
        .args([
            "board",
            "new",
            "nocheckout",
            "--platform",
            "freertos",
            "--out-of-tree",
        ])
        .arg(&boards)
        .current_dir(tmp.path())
        .env_remove("NROS_WORKSPACE_ROOT")
        .output()
        .expect("run nros board new");
    assert!(
        out.status.success(),
        "--out-of-tree must scaffold with no checkout: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let dir = boards.join("nros-board-nocheckout");
    assert!(dir.join("nros-board.toml").is_file());
    assert!(dir.join("package.xml").is_file());
}

/// Issue 2292 -- `nros new board <name> --for-platform <p>` is the second
/// spelling of a board scaffold, and its board must resolve through the same
/// catalog `nros board new`'s does. It wrote no `package.xml`, so every
/// catalog reader refused the board it had just reported scaffolded.
///
/// An in-tree platform on purpose; the out-of-tree road is
/// `an_out_of_tree_platform_and_its_board_resolve` (issue 2310).
#[test]
fn new_board_for_platform_resolves_through_the_catalog() {
    let tmp = outside_every_checkout();
    let out = Command::new(nros())
        .args(["new", "board", "acme", "--for-platform", "nuttx", "--into"])
        .arg(tmp.path())
        .current_dir(tmp.path())
        .env_remove("NROS_WORKSPACE_ROOT")
        .output()
        .expect("run nros new board");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let dir = tmp.path().join("nros-board-acme");
    let xml = std::fs::read_to_string(dir.join("package.xml")).expect("package.xml written");
    assert!(
        xml.contains(r#"<nano_ros_provides kind="board" name="acme"/>"#),
        "{xml}"
    );

    let out = Command::new(nros())
        .args(["board", "info", "acme", "--workspace"])
        .arg(repo_root())
        .env("NROS_EXTRA_BOARD_PATH", tmp.path())
        .output()
        .expect("run nros board info");
    assert!(
        out.status.success(),
        "a board from `nros new board --for-platform` must resolve: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Issue 2292 -- the two spellings write one package, not two templates that
/// drift. Byte-compare what each writes for the same name and platform.
#[test]
fn both_board_scaffold_spellings_write_the_same_package() {
    let a = outside_every_checkout();
    let b = outside_every_checkout();
    let out = Command::new(nros())
        .args([
            "board",
            "new",
            "same",
            "--platform",
            "freertos",
            "--out-of-tree",
        ])
        .arg(a.path())
        .current_dir(a.path())
        .env_remove("NROS_WORKSPACE_ROOT")
        .output()
        .expect("run nros board new");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out = Command::new(nros())
        .args([
            "new",
            "board",
            "same",
            "--for-platform",
            "freertos",
            "--into",
        ])
        .arg(b.path())
        .current_dir(b.path())
        .env_remove("NROS_WORKSPACE_ROOT")
        .output()
        .expect("run nros new board");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    for f in ["nros-board.toml", "package.xml"] {
        let ta = std::fs::read_to_string(a.path().join("nros-board-same").join(f)).unwrap();
        let tb = std::fs::read_to_string(b.path().join("nros-board-same").join(f)).unwrap();
        assert_eq!(ta, tb, "{f} differs between the two scaffold spellings");
    }
}

/// Issue 2292 -- `nros new` takes no Zephyr board id, so a Zephyr board from
/// that spelling is refused, naming the spelling that takes one.
#[test]
fn new_board_for_zephyr_names_the_verb_that_takes_a_board_id() {
    let tmp = outside_every_checkout();
    let out = Command::new(nros())
        .args(["new", "board", "zb", "--for-platform", "zephyr", "--into"])
        .arg(tmp.path())
        .current_dir(tmp.path())
        .output()
        .expect("run nros new board");
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("nros board new zb --platform zephyr --west-board"),
        "{err}"
    );
    assert!(!tmp.path().join("nros-board-zb").exists());
}

/// The out-of-tree road is the user's, and a user has neither the checkout's
/// `just` recipes nor a reason to enter the `nros dev` tier. Both spellings
/// reach it, so both are checked.
#[test]
fn the_out_of_tree_next_steps_name_no_contributor_step() {
    let a = outside_every_checkout();
    let b = outside_every_checkout();
    let outs = [
        Command::new(nros())
            .args([
                "board",
                "new",
                "userboard",
                "--platform",
                "freertos",
                "--out-of-tree",
            ])
            .arg(a.path())
            .current_dir(a.path())
            .output()
            .expect("run nros board new"),
        Command::new(nros())
            .args([
                "new",
                "board",
                "userboard",
                "--for-platform",
                "freertos",
                "--into",
            ])
            .arg(b.path())
            .current_dir(b.path())
            .output()
            .expect("run nros new board"),
    ];
    for out in outs {
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            !text.contains("just ") && !text.contains("nros dev"),
            "a user-road next step names a contributor step:\n{text}"
        );
        // Issue 2294 — and it names the user's own check.
        assert!(
            text.contains("nros check userboard"),
            "the next steps name no check:\n{text}"
        );
    }
}

/// Issue 2294 — the check the scaffold names runs: `nros check <key>` from
/// a directory in no workspace finds the out-of-tree board by its key and by
/// its package name, and a name nothing answers to fails naming the forms.
#[test]
fn nros_check_runs_the_board_check_on_a_scaffolded_board() {
    let tmp = tempfile::tempdir().expect("tempdir");
    scaffold(tmp.path(), &["vendorboard", "--platform", "freertos"]);
    let cwd = outside_every_checkout();
    let check = |name: &str| {
        Command::new(nros())
            .args(["check", name, "--nano-ros-path"])
            .arg(repo_root())
            .env("NROS_EXTRA_BOARD_PATH", tmp.path())
            .current_dir(cwd.path())
            .output()
            .expect("run nros check")
    };
    for name in ["vendorboard", "nros_board_vendorboard"] {
        let out = check(name);
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            out.status.success(),
            "`nros check {name}` failed: {}{stdout}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            stdout.contains("board `vendorboard`: OK"),
            "{name}: {stdout}"
        );
    }
    let out = check("no_such_package");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success());
    assert!(
        stderr.contains("no runner, command or board package answers to it"),
        "{stderr}"
    );
}

/// Issue 2310's "Done when", end to end: a porter scaffolds a platform and a
/// board for it, and the board resolves with no edit outside their own
/// directory. User ruling 2026-11-12 (option 3): the platform's `inherits`
/// chain names its built-in family. Without `--inherits` the board is refused,
/// naming the line that fixes it.
#[test]
fn an_out_of_tree_platform_and_its_board_resolve() {
    let run = |dir: &std::path::Path, args: &[&str]| {
        Command::new(nros())
            .args(args)
            .current_dir(dir)
            .env_remove("NROS_WORKSPACE_ROOT")
            .env("NROS_EXTRA_BOARD_PATH", dir)
            .output()
            .expect("run nros")
    };
    let into = |dir: &std::path::Path, args: &[&str]| -> Vec<String> {
        let mut v: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        v.push("--into".into());
        v.push(dir.display().to_string());
        v
    };
    let info = |dir: &std::path::Path| {
        let root = repo_root();
        run(
            dir,
            &[
                "board",
                "info",
                "acme",
                "--workspace",
                root.to_str().unwrap(),
            ],
        )
    };

    for (inherits, family) in [(Some("bare-metal"), "bare-metal"), (None, "")] {
        let tmp = outside_every_checkout();
        let mut plat = vec!["new", "platform", "acmeos"];
        if let Some(f) = inherits {
            plat.extend(["--inherits", f]);
        }
        for args in [
            plat,
            vec!["new", "board", "acme", "--for-platform", "acmeos"],
        ] {
            let args = into(tmp.path(), &args);
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            let out = run(tmp.path(), &args);
            assert!(
                out.status.success(),
                "{args:?}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
        let out = info(tmp.path());
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        if inherits.is_some() {
            assert!(out.status.success(), "the board must resolve: {stderr}");
            assert!(stdout.contains(r#""platform": "acmeos""#), "{stdout}");
            assert!(
                stdout.contains(&format!(r#""platform_family": "{family}""#)),
                "{stdout}"
            );
        } else {
            assert!(
                !out.status.success(),
                "a platform with no family must refuse: {stdout}"
            );
            assert!(
                stderr.contains("inherits = \"<family>\""),
                "the refusal names the fix: {stderr}"
            );
        }
    }
}
