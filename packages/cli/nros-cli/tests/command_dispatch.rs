//! phase-527 W4 (RFC-0119 D7) — command dispatch through the real binary.
//!
//! `nros <verb>` with no built-in match runs the `bin` of the command package
//! announcing `<verb>`, with its arguments unchanged and `NROS_WS_ROOT`,
//! `NROS_TOOLCHAIN` and `NROS_CLI` set, and passes its exit status through. A
//! command announcing a built-in or reserved verb is refused at discovery.
//!
//! The fixture workspace is the tracked
//! `nros-cli-core/tests/fixtures/extension_packages/dispatch/ws` (kept out of
//! the nano-ros tree's own scan by a `COLCON_IGNORE` above it), copied into a
//! tempdir. Nothing is compiled: the "built" command is a shell script placed
//! where `nros build` puts a CMake host tool's executable
//! (`builder::host_tool::candidates`), so the test exercises the lookup and
//! the exec, which is what W4 adds; W2's tests cover the build itself.
#![cfg(unix)]

use std::{
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Output},
};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../nros-cli-core/tests/fixtures/extension_packages/dispatch/ws")
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let dst = to.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            copy_dir(&e.path(), &dst);
        } else {
            std::fs::copy(e.path(), &dst).unwrap();
        }
    }
}

/// The fixture workspace in a tempdir, canonical (`NROS_WS_ROOT` is).
fn workspace() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let ws = tmp.path().join("ws");
    copy_dir(&fixture(), &ws);
    let ws = ws.canonicalize().unwrap();
    (tmp, ws)
}

/// "Build" `package`'s `bin`: a script that echoes its arguments and the three
/// variables, then exits 7.
fn build(ws: &Path, package: &str, bin: &str) {
    let exe = ws
        .join("build/nros/host")
        .join(package)
        .join("cmake")
        .join(bin);
    std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
    std::fs::write(
        &exe,
        "#!/bin/sh\n\
         for a in \"$@\"; do printf 'arg:%s\\n' \"$a\"; done\n\
         printf 'ws:%s\\ntoolchain:%s\\ncli:%s\\n' \"$NROS_WS_ROOT\" \"$NROS_TOOLCHAIN\" \"$NROS_CLI\"\n\
         exit 7\n",
    )
    .unwrap();
    std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// Add a command package announcing the built-in verb `build`. Written into
/// the tempdir rather than tracked, because `check-provider-announcements` A7
/// refuses every tracked `package.xml` that announces one, fixtures included.
fn announce_build(ws: &Path) {
    let dir = ws.join("src/dispatch_build");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("package.xml"),
        "<?xml version=\"1.0\"?>\n<package format=\"3\"><name>dispatch_build</name>\
         <version>0.1.0</version><description>d</description>\
         <maintainer email=\"noreply@example.com\">nano-ros</maintainer>\
         <license>Apache-2.0</license><export><build_type>nros_cargo</build_type>\
         <nano_ros_provides kind=\"command\" name=\"build\"/></export></package>\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"dispatch_build\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
         [[bin]]\nname = \"nros-fixture-build\"\npath = \"src/main.rs\"\n\n[workspace]\n",
    )
    .unwrap();
    std::fs::write(dir.join("src/main.rs"), "fn main() {}\n").unwrap();
    std::fs::write(
        dir.join("nros-command.toml"),
        "protocol = 1\nsummary = \"Claims a built-in verb\"\nbin = \"nros-fixture-build\"\n",
    )
    .unwrap();
}

fn nros(ws: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_nros"))
        .args(args)
        .current_dir(ws)
        .env_remove("NROS_PACKAGE_PATH")
        .env_remove("NROS_WS_ROOT")
        .env_remove("NROS_TOOLCHAIN")
        .env_remove("NROS_CLI")
        .output()
        .unwrap()
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

#[test]
fn a_command_runs_with_its_args_unchanged_the_three_variables_and_its_status() {
    let (_tmp, ws) = workspace();
    build(&ws, "dispatch_greet", "nros-fixture-greet");
    let o = nros(
        &ws,
        &[
            "fixture-greet",
            "config",
            "--flag",
            "two words",
            "--help",
            "",
        ],
    );
    let out = text(&o.stdout);
    assert_eq!(
        o.status.code(),
        Some(7),
        "status passes through: {out}{}",
        text(&o.stderr)
    );
    let cli = PathBuf::from(env!("CARGO_BIN_EXE_nros"))
        .canonicalize()
        .unwrap();
    let want = format!(
        "arg:config\narg:--flag\narg:two words\narg:--help\narg:\n\
         ws:{}\ntoolchain:{}\ncli:{}\n",
        ws.display(),
        env!("CARGO_PKG_VERSION"),
        cli.display()
    );
    assert_eq!(out, want);
}

#[test]
fn a_dev_tier_command_runs_under_nros_dev_only() {
    let (_tmp, ws) = workspace();
    build(&ws, "dispatch_dev_greet", "nros-fixture-dev-greet");
    let o = nros(&ws, &["dev", "fixture-dev-greet", "a"]);
    assert_eq!(o.status.code(), Some(7), "{}", text(&o.stderr));
    assert!(
        text(&o.stdout).starts_with("arg:a\n"),
        "{}",
        text(&o.stdout)
    );

    let o = nros(&ws, &["fixture-dev-greet", "a"]);
    assert_eq!(o.status.code(), Some(3));
    let err = text(&o.stderr);
    assert!(
        err.contains("provides it as `nros dev fixture-dev-greet`"),
        "{err}"
    );
    let o = nros(&ws, &["dev", "fixture-greet"]);
    assert_eq!(o.status.code(), Some(3));
    assert!(
        text(&o.stderr).contains("as `nros fixture-greet`"),
        "{}",
        text(&o.stderr)
    );
}

#[test]
fn an_unbuilt_command_is_refused_naming_its_build() {
    let (_tmp, ws) = workspace();
    let o = nros(&ws, &["fixture-greet"]);
    assert_eq!(o.status.code(), Some(3));
    let err = text(&o.stderr);
    assert!(
        err.contains(
            "`nros fixture-greet` is provided by package `dispatch_greet`, which is not built"
        ),
        "{err}"
    );
    assert!(
        err.contains("nros build --packages-select dispatch_greet"),
        "{err}"
    );
}

#[test]
fn a_command_named_build_is_refused_at_discovery_and_the_built_in_runs() {
    let (_tmp, ws) = workspace();
    announce_build(&ws);
    build(&ws, "dispatch_build", "nros-fixture-build");
    // Listed nowhere, refused by name in `nros help --all`.
    let o = nros(&ws, &["help", "--all"]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    let (out, err) = (text(&o.stdout), text(&o.stderr));
    assert!(
        err.contains("announces command `build`, which is a built-in or reserved `nros` verb"),
        "{err}"
    );
    assert!(!out.contains("dispatch_build"), "{out}");
    // `nros build` is still the built-in: the script would print `ws:`.
    let o = nros(&ws, &["build", "--help"]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert!(!text(&o.stdout).contains("ws:"), "{}", text(&o.stdout));
}

#[test]
fn help_all_lists_extension_verbs_with_summary_and_package() {
    let (_tmp, ws) = workspace();
    let o = nros(&ws, &["help", "--all"]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    let out = text(&o.stdout);
    let section = out
        .split("Extension commands (from packages):\n")
        .nth(1)
        .unwrap_or_else(|| panic!("no extension section:\n{out}"));
    let row = |spelling: &str, rest: &str| {
        section
            .lines()
            .any(|l| l.trim_start().starts_with(spelling) && l.trim_end().ends_with(rest))
    };
    assert!(
        row(
            "nros fixture-greet",
            "Print its arguments (a test fixture) (package dispatch_greet)"
        ),
        "{section}"
    );
    assert!(
        row("nros dev fixture-dev-greet", "(package dispatch_dev_greet)"),
        "{section}"
    );
}

#[test]
fn an_unknown_verb_no_package_announces_keeps_clap_s_error() {
    let (_tmp, ws) = workspace();
    let o = nros(&ws, &["no-such-verb-anywhere"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(
        text(&o.stderr).contains("unrecognized subcommand 'no-such-verb-anywhere'"),
        "{}",
        text(&o.stderr)
    );
}
