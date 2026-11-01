//! The two command tiers as a user reaches them, through the real `nros`
//! binary (RFC-0117, phase-526 W2).
//!
//! `nros-cli-core`'s `cmd::user` tests hold the alias table and the help
//! groups to the parser. This file covers what only the binary decides: that
//! `nros --help` prints the grouped list, that an old user spelling stays
//! silent when stderr is piped, and that every new spelling builds its clap
//! command with the binary's `propagate_version` setting.

use std::process::Command;

fn nros() -> Command {
    Command::new(env!("CARGO_BIN_EXE_nros"))
}

fn run(args: &[&str]) -> (bool, String, String) {
    let out = nros().args(args).output().expect("failed to exec nros");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn top_level_help_shows_the_groups_in_order() {
    let (ok, help, err) = run(&["--help"]);
    assert!(ok, "{err}");
    // The headings after the usage line; the long description above it has
    // a "Quick start:" block of its own.
    let headings: Vec<&str> = help
        .lines()
        .skip_while(|l| !l.starts_with("Usage:"))
        .filter(|l| !l.starts_with(' ') && l.ends_with(':'))
        .collect();
    assert_eq!(
        headings,
        [
            "Start:",
            "Build:",
            "Run on a target:",
            "Workspace:",
            "Interfaces:",
            "Inspect:",
            "Toolchain:",
            "Other:",
            "Options:"
        ],
        "{help}"
    );
    for hidden in [
        "image-facts",
        "sdk-root",
        "  ws ",
        "  dev ",
        "  init ",
        "  store ",
    ] {
        assert!(
            !help.contains(hidden),
            "`{hidden}` in `nros --help`:\n{help}"
        );
    }
}

#[test]
fn help_all_lists_the_dev_tier() {
    let (ok, help, err) = run(&["help", "--all"]);
    assert!(ok, "{err}");
    assert!(help.contains("  dev "), "{help}");
    assert!(help.contains("codegen"), "{help}");
}

#[test]
fn every_new_user_spelling_parses() {
    for args in [
        vec!["setup", "presets", "--help"],
        vec!["toolchain", "list", "--help"],
        vec!["toolchain", "gc", "--help"],
        vec!["toolchain", "remove", "--help"],
        vec!["interface", "generate", "--help"],
        vec!["interface", "list", "--help"],
        vec!["interface", "show", "--help"],
        vec!["list", "--help"],
        vec!["graph", "--help"],
        vec!["clean", "--help"],
    ] {
        let (ok, _, err) = run(&args);
        assert!(ok, "`nros {}` failed:\n{err}", args.join(" "));
    }
}

/// Piped stderr is the case a script sees, so the old spelling must print
/// exactly what the new one does there, note included (none).
#[test]
fn an_old_user_spelling_is_silent_when_piped() {
    let tmp = tempfile::tempdir().unwrap();
    let pkg = tmp.path().join("src/demo");
    std::fs::create_dir_all(&pkg).unwrap();
    std::fs::write(
        pkg.join("package.xml"),
        "<package format=\"3\"><name>demo</name><version>0.0.0</version>\
         <description>d</description><maintainer email=\"a@b.c\">a</maintainer>\
         <license>MIT</license></package>",
    )
    .unwrap();
    let ws = tmp.path().to_str().unwrap();
    let old = run(&["ws", "list", ws]);
    let new = run(&["list", ws]);
    assert!(old.0 && new.0, "{}\n{}", old.2, new.2);
    assert_eq!(old.1, new.1, "stdout differs between `ws list` and `list`");
    assert_eq!(old.2, new.2, "stderr differs between `ws list` and `list`");
    assert!(!old.2.contains("note:"), "{}", old.2);
}

#[test]
fn a_reserved_verb_refuses_and_names_what_to_do() {
    for verb in ["flash", "monitor", "bench", "debug"] {
        let (ok, _, err) = run(&[verb, "talker"]);
        assert!(!ok, "`nros {verb}` succeeded");
        assert!(err.contains("is reserved"), "{err}");
        assert!(err.contains("nros build"), "{err}");
    }
    // phase-528 W1 — `run` resolves its plan with `--dry-run`; launching is
    // W4, and until then it refuses naming what works.
    let (ok, _, err) = run(&["run", "talker"]);
    assert!(!ok, "`nros run` succeeded");
    assert!(
        err.contains("phase-528 W4") && err.contains("--dry-run"),
        "{err}"
    );
}
