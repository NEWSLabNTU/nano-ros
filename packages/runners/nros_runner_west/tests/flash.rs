//! `nros-runner-west flash` end to end, with a stand-in `west` from the plan's
//! tools that records its arguments. No board and no Zephyr tree.

use std::{
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
};

const BIN: &str = env!("CARGO_BIN_EXE_nros-runner-west");

struct Dir(PathBuf);
impl Dir {
    fn new(tag: &str) -> Self {
        let d = std::env::temp_dir().join(format!("nros-runner-west-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(d.join("build/zephyr")).unwrap();
        Dir(d)
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A `west` that writes its arguments, one per line, and exits `code`.
fn fake_west(dir: &Dir, code: i32) -> PathBuf {
    let p = dir.0.join("west");
    let log = dir.0.join("argv");
    std::fs::write(
        &p,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\nexit {code}\n",
            log.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    p
}

fn plan(dir: &Dir, west: &Path, args: &str, passthrough: &str) -> PathBuf {
    let p = dir.0.join("plan.json");
    std::fs::write(
        &p,
        format!(
            r#"{{"image":"i","verb":"flash","runner":{{"package":"nros_runner_west","name":"west"}},
            "target":{{"flasher":"west"}},"console":{{"kind":"serial"}},
            "build_dir":"{}","tools":{{"west":"{}"}},"args":{{{args}}},"passthrough":[{passthrough}]}}"#,
            dir.0.join("build").display(),
            west.display()
        ),
    )
    .unwrap();
    p
}

fn flash(plan: &Path) -> std::process::Output {
    Command::new(BIN)
        .args(["flash", "--plan"])
        .arg(plan)
        .output()
        .unwrap()
}

const YAML: &str = "runners:\n- openocd\n- jlink\nflash-runner: openocd\n";

#[test]
fn west_flash_gets_the_boards_default_runner() {
    let d = Dir::new("default");
    std::fs::write(d.0.join("build/zephyr/runners.yaml"), YAML).unwrap();
    let out = flash(&plan(&d, &fake_west(&d, 0), "", ""));
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let argv = std::fs::read_to_string(d.0.join("argv")).unwrap();
    let build = d.0.join("build");
    assert_eq!(
        argv,
        format!("flash\n-d\n{}\n-r\nopenocd\n", build.display())
    );
}

#[test]
fn the_requested_runner_and_arguments_after_dashdash_pass_through() {
    let d = Dir::new("jlink");
    std::fs::write(d.0.join("build/zephyr/runners.yaml"), YAML).unwrap();
    let out = flash(&plan(
        &d,
        &fake_west(&d, 0),
        r#""runner":"jlink""#,
        r#""--speed","4000""#,
    ));
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let argv = std::fs::read_to_string(d.0.join("argv")).unwrap();
    assert!(argv.ends_with("-r\njlink\n--\n--speed\n4000\n"), "{argv}");
}

#[test]
fn a_failed_flash_an_unconfigured_runner_and_no_build_are_exit_2() {
    let d = Dir::new("fail");
    let west = fake_west(&d, 1);
    // No runners.yaml: the build never finished.
    let out = flash(&plan(&d, &west, "", ""));
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("runners.yaml"));

    std::fs::write(d.0.join("build/zephyr/runners.yaml"), YAML).unwrap();
    let out = flash(&plan(&d, &west, r#""runner":"pyocd""#, ""));
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("openocd, jlink"));

    // west itself fails: the board was not flashed.
    let out = flash(&plan(&d, &west, "", ""));
    assert_eq!(out.status.code(), Some(2));
}
