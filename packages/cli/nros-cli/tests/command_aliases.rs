//! phase-527 W5 (RFC-0119 D8) — the built-ins that became command packages
//! keep their old spellings as hidden aliases that run the package.
//!
//! `nros ws router-config …` runs `nros router config …` and `nros
//! generate-px4-msgs …` runs `nros px4 msgs …`, with the arguments unchanged.
//! The test does not need the real packages built: the workspace declares an
//! override of each verb (`nros.toml [workspace.overrides]`, RFC-0119 D4)
//! with a fixture package whose "built" executable is a shell script that
//! echoes its argv. So it measures the alias's wiring, which is what this
//! work item adds; the byte identity of the outputs was measured on the real
//! packages and is recorded in the phase doc.
#![cfg(unix)]

use std::{
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Output},
};

const PX: &str = "<?xml version=\"1.0\"?>\n<package format=\"3\"><name>{pkg}</name>\
    <version>0.0.0</version><description>d</description>\
    <maintainer email=\"noreply@example.com\">nano-ros</maintainer>\
    <license>Apache-2.0</license><export><build_type>nros_cargo</build_type>\
    <nano_ros_provides kind=\"command\" name=\"{verb}\"/></export></package>\n";

/// A command package `pkg` announcing `verb`, "built" as a script printing
/// `<verb>:` and then each argument on its own line.
fn package(ws: &Path, pkg: &str, verb: &str, bin: &str) {
    let dir = ws.join("src").join(pkg);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("package.xml"),
        PX.replace("{pkg}", pkg).replace("{verb}", verb),
    )
    .unwrap();
    std::fs::write(
        dir.join("Cargo.toml"),
        format!(
            "[package]\nname = \"{pkg}\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n\
             [[bin]]\nname = \"{bin}\"\npath = \"src/main.rs\"\n\n[workspace]\n"
        ),
    )
    .unwrap();
    std::fs::write(dir.join("src/main.rs"), "fn main() {}\n").unwrap();
    std::fs::write(
        dir.join("nros-command.toml"),
        "protocol = 1\nsummary = \"fixture\"\n",
    )
    .unwrap();
    let exe = ws.join("build/nros/host").join(pkg).join("cmake").join(bin);
    std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
    std::fs::write(
        &exe,
        format!("#!/bin/sh\necho {verb}:\nfor a in \"$@\"; do printf '%s\\n' \"$a\"; done\n"),
    )
    .unwrap();
    std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn workspace() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let ws = tmp.path().join("ws");
    package(&ws, "fake_zenoh_tools", "router", "fake-zenoh-tools");
    package(&ws, "fake_px4_tools", "px4", "fake-px4-tools");
    // Declared, so they replace the nano-ros tree's own packages when the
    // tree is the underlay; with no such underlay they override nothing,
    // which is a warning, never a refusal.
    std::fs::write(
        ws.join("nros.toml"),
        "[workspace.overrides]\nrouter = \"src/fake_zenoh_tools\"\npx4 = \"src/fake_px4_tools\"\n",
    )
    .unwrap();
    let ws = ws.canonicalize().unwrap();
    (tmp, ws)
}

fn nros(ws: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_nros"))
        .args(args)
        .current_dir(ws)
        .env_remove("NROS_PACKAGE_PATH")
        .output()
        .unwrap()
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

#[test]
fn ws_router_config_runs_router_config_with_its_args() {
    let (_tmp, ws) = workspace();
    // `nros dev ws router-config` never existed, so `nros ws` is the only
    // old spelling.
    let o = nros(
        &ws,
        &[
            "ws",
            "router-config",
            "--model",
            "m.yaml",
            "--format",
            "json5",
            "--help",
        ],
    );
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(
        stdout(&o),
        "router:\nconfig\n--model\nm.yaml\n--format\njson5\n--help\n"
    );
}

#[test]
fn generate_px4_msgs_runs_px4_msgs_with_its_args() {
    let (_tmp, ws) = workspace();
    let o = nros(
        &ws,
        &[
            "generate-px4-msgs",
            "--px4",
            "p",
            "-o",
            "out",
            "--lang",
            "cpp",
        ],
    );
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(stdout(&o), "px4:\nmsgs\n--px4\np\n-o\nout\n--lang\ncpp\n");
}

#[test]
fn the_new_spellings_reach_the_same_packages() {
    let (_tmp, ws) = workspace();
    let o = nros(&ws, &["router", "config", "--model", "m.yaml"]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(stdout(&o), "router:\nconfig\n--model\nm.yaml\n");
    let o = nros(&ws, &["px4", "msgs"]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(stdout(&o), "px4:\nmsgs\n");
}

#[test]
fn the_old_spellings_are_hidden_from_help() {
    let (_tmp, ws) = workspace();
    let help = stdout(&nros(&ws, &["--help"]));
    assert!(!help.contains("generate-px4-msgs"), "{help}");
    let help = stdout(&nros(&ws, &["ws", "--help"]));
    assert!(!help.contains("router-config"), "{help}");
}
