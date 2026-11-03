//! The `west` runner's one question, shared with the `fvp` runner: which
//! `west build -t <target>` boots a finished Zephyr build on a given
//! emulator.
//!
//! Zephyr owns the answer. A board's `board.cmake` declares the emulators it
//! supports, and the build emits one `run_<emulator>` ninja target per
//! emulator (plus `run`, which aliases the board's default). This crate reads
//! them from the build's own `build.ninja` and never restates a board's list.

use std::path::{Path, PathBuf};

use nros_extension::plan::RunPlan;

/// The `run_<emulator>` targets a finished west build declares, in the order
/// `build.ninja` lists them. `None` when the build dir has no `build.ninja`
/// (never configured).
#[must_use]
pub fn run_targets(build_dir: &Path) -> Option<Vec<String>> {
    let ninja = std::fs::read_to_string(build_dir.join("build.ninja")).ok()?;
    let mut out: Vec<String> = Vec::new();
    for emu in ninja
        .lines()
        .filter_map(|l| l.strip_prefix("build run_"))
        .filter_map(|r| r.split(':').next())
    {
        if !out.iter().any(|e| e == emu) {
            out.push(emu.to_string());
        }
    }
    Some(out)
}

/// The `-t` target that boots `emulator` (Zephyr's own name for it, such as
/// `qemu`, `renode` or `armfvp`), given what the build declares.
///
/// # Errors
///
/// The build declares run targets and none is for `emulator`: a message
/// naming the ones it has.
pub fn target_for(emulator: &str, declared: Option<&[String]>) -> Result<String, String> {
    match declared {
        // Not configured yet: `west build -t run` configures it, and Zephyr's
        // default emulator for the board is the only one we can name.
        None => Ok("run".to_string()),
        Some([]) => Err(
            "the west build declares no `run_<emulator>` target: the board's own Zephyr \
             definition names no emulator, so west cannot boot it"
                .to_string(),
        ),
        Some(list) if list.iter().any(|e| e == emulator) => Ok(format!("run_{emulator}")),
        Some(list) => Err(format!(
            "the west build declares run targets for {} and none for `{emulator}`",
            list.join(", ")
        )),
    }
}

/// The plan's build directory, which every west-driven runner needs.
///
/// # Errors
///
/// The plan carries none.
pub fn build_dir(plan: &RunPlan) -> Result<PathBuf, String> {
    plan.build_dir.clone().ok_or_else(|| {
        "the plan carries no `build_dir`; this runner boots a west build directory".to_string()
    })
}

/// `west`, from the plan's tools when nros resolved one, else the `west` of
/// the environment the Zephyr workspace was set up in (west is that
/// workspace's own tool, as it is for every `west build` the build road ran).
#[must_use]
pub fn west_program(plan: &RunPlan) -> PathBuf {
    plan.tools
        .get("west")
        .cloned()
        .unwrap_or_else(|| PathBuf::from("west"))
}

/// The `west build -d <dir> -t <target>` argument vector, without `west`.
#[must_use]
pub fn run_argv(build_dir: &Path, target: &str) -> Vec<String> {
    vec![
        "build".to_string(),
        "-d".to_string(),
        build_dir.display().to_string(),
        "-t".to_string(),
        target.to_string(),
    ]
}

/// Refuse arguments after `--`: `west build -- <args>` hands them to CMake
/// and reconfigures the build, and an emulator run target has no way to pass
/// arguments to the image.
///
/// # Errors
///
/// The plan carries passthrough arguments.
pub fn no_passthrough(plan: &RunPlan) -> Result<(), String> {
    if plan.passthrough.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "arguments after `--` ({}) cannot reach an image booted by a west run target; \
             `west build -- <args>` would hand them to CMake instead",
            plan.passthrough.join(" ")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_targets_come_from_the_build_ninja() {
        let dir = std::env::temp_dir().join(format!("nros-runner-west-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(run_targets(&dir), None);
        std::fs::write(
            dir.join("build.ninja"),
            "build run: phony run_qemu\nbuild run_qemu: CUSTOM_COMMAND x\n\
             build run_renode: CUSTOM_COMMAND y\nbuild run_qemu: phony\n",
        )
        .unwrap();
        assert_eq!(
            run_targets(&dir).unwrap(),
            vec!["qemu".to_string(), "renode".to_string()]
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_target_is_the_emulators_own_or_a_refusal_naming_what_exists() {
        let l = vec!["qemu".to_string(), "renode".to_string()];
        assert_eq!(target_for("renode", Some(&l)).unwrap(), "run_renode");
        assert_eq!(target_for("qemu", None).unwrap(), "run");
        let e = target_for("wokwi", Some(&l)).unwrap_err();
        assert!(e.contains("qemu, renode") && e.contains("wokwi"), "{e}");
        assert!(target_for("qemu", Some(&[])).is_err());
    }

    #[test]
    fn the_argv_is_build_dir_and_target() {
        assert_eq!(
            run_argv(Path::new("/b"), "run_qemu"),
            ["build", "-d", "/b", "-t", "run_qemu"]
        );
    }
}
