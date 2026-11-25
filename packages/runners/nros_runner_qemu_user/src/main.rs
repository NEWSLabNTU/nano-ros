// RFC-0118 D4b, issue 2852.
//! `nros-runner-qemu-user`: the `qemu-user` runner, `run` only.
//!
//! Runs a cross-built hosted Linux image under QEMU's user mode:
//! `qemu-<arch> <elf> <args>`, for a board whose `[board.run]` reads
//! `emulator = "qemu-user:<arch>"` (the musl `linux-aarch64-musl` board). It is the native runner's shape with the program under the emulator:
//! the image's stdout and stderr are its console (both on this runner's
//! stdout, as the protocol grades stdout only), the plan's passthrough is its
//! argv, and a killed verb takes it down.
//!
//! The emulator is the plan's: `tools.qemu-user` is the absolute path `nros`
//! resolved (the host's `qemu-<arch>` or `qemu-<arch>-static`; the SDK's QEMU
//! is system emulation only). A plan without one is refused, naming what to
//! install, rather than guessed from `PATH` here.
//!
//! What a run is evidence for: the image's own user space. Every system call
//! is answered by the host's kernel, so timers, scheduling and preemption are
//! the host's, never the target's.

use std::{
    path::{Path, PathBuf},
    process::{Command, ExitCode, Stdio},
};

use nros_runner_lifetime as lifetime;

use nros_extension::{
    describe::Describe,
    exit::RunnerExit,
    plan::{RunPlan, Verb},
    runner,
};

/// The emulator kind this runner serves.
const KIND: &str = "qemu-user";

/// The oldest QEMU whose user mode runs a nano-ros image. The POSIX port's
/// mutexes are priority-inheriting, which musl and glibc lock with
/// `FUTEX_LOCK_PI`; QEMU's user mode answers that with `ENOSYS` before 7.2, so
/// the image's first lock fails and its session never opens (measured on 6.2,
/// Ubuntu 22.04's `qemu-user-static`).
const MIN_QEMU: (u32, u32) = (7, 2);

/// `(major, minor)` from `qemu-<arch> --version`'s first line
/// (`qemu-aarch64 version 6.2.0 (Debian ...)`).
fn parse_version(text: &str) -> Option<(u32, u32)> {
    let rest = text.lines().next()?.split("version ").nth(1)?;
    let mut parts = rest.split(|c: char| !c.is_ascii_digit());
    Some((parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
}

/// Why `emulator` is too old to run an image, or `None` when it is new enough
/// or its version cannot be read (an unreadable one is let run, not guessed).
fn too_old(emulator: &Path, version_text: &str) -> Option<String> {
    let v = parse_version(version_text)?;
    (v < MIN_QEMU).then(|| {
        format!(
            "{} is QEMU {}.{}, and QEMU's user mode before {}.{} answers FUTEX_LOCK_PI \
             (a priority-inheriting mutex, which every nano-ros image locks) with ENOSYS, so \
             the image would fail at its first lock. Install QEMU {}.{} or newer (Ubuntu \
             24.04's `qemu-user-static` is 8.2).",
            emulator.display(),
            v.0,
            v.1,
            MIN_QEMU.0,
            MIN_QEMU.1,
            MIN_QEMU.0,
            MIN_QEMU.1
        )
    })
}

fn describe() -> Describe {
    Describe::builder().verb(Verb::Run).need("elf").build()
}

fn main() -> ExitCode {
    runner::main(&describe(), |_verb, plan| run(plan))
}

/// The architecture word of the plan's `qemu-user:<arch>` target.
fn arch(plan: &RunPlan) -> Result<&str, String> {
    let emulator = plan.target.emulator.as_deref().ok_or_else(|| {
        format!(
            "the plan's target names no emulator; the {KIND} runner runs `{KIND}:<arch>` images"
        )
    })?;
    match emulator.split_once(':') {
        Some((KIND, a)) if !a.is_empty() => Ok(a),
        _ => Err(format!(
            "the plan's emulator is `{emulator}`; the {KIND} runner runs `{KIND}:<arch>` images"
        )),
    }
}

/// The program and its arguments: the emulator, the image, then the
/// passthrough.
fn command_line(plan: &RunPlan, exe: &Path) -> Result<(PathBuf, Vec<String>), String> {
    let arch = arch(plan)?;
    let emulator = plan.tools.get(KIND).cloned().ok_or_else(|| {
        format!(
            "the plan's tools name no `{KIND}`: QEMU user mode for `{arch}` (`qemu-{arch}` or \
             `qemu-{arch}-static`) was not found when the plan was made. Install the distro's \
             `qemu-user-static` (or `qemu-user`) package."
        )
    })?;
    let mut args = vec![exe.display().to_string()];
    args.extend(plan.passthrough.iter().cloned());
    Ok((emulator, args))
}

fn run(plan: &RunPlan) -> RunnerExit {
    let Some(exe) = plan.artifacts.get("elf") else {
        return runner::fail("the plan carries no `elf` artifact to run");
    };
    let (program, args) = match command_line(plan, exe) {
        Ok(c) => c,
        Err(e) => return runner::fail(e),
    };
    if let Ok(out) = Command::new(&program).arg("--version").output()
        && let Some(why) = too_old(&program, &String::from_utf8_lossy(&out.stdout))
    {
        return runner::fail(why);
    }
    let mut cmd = Command::new(program);
    cmd.args(args);
    match stdout_as_stdio() {
        Ok(s) => {
            cmd.stderr(s);
        }
        Err(e) => return runner::fail(format!("duplicating stdout: {e}")),
    }
    runner::exec(lifetime::die_with_runner(&mut cmd))
}

/// This process's stdout, as a child's stdio handle.
fn stdout_as_stdio() -> std::io::Result<Stdio> {
    use std::os::fd::AsFd;
    Ok(Stdio::from(std::io::stdout().as_fd().try_clone_to_owned()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(emulator: &str, tools: &str) -> RunPlan {
        RunPlan::from_json(&format!(
            r#"{{"image":"i","verb":"run","runner":{{"package":"nros_runner_qemu_user","name":"qemu-user"}},
            "target":{{"emulator":"{emulator}"}},"console":{{"kind":"stdio"}},
            "artifacts":{{"elf":"/b/talker"}},"tools":{tools},
            "passthrough":["--nros-locator","tcp/10.0.2.2:7447"]}}"#
        ))
        .unwrap()
    }

    #[test]
    fn describe_agrees_with_the_descriptor() {
        runner::descriptor_agrees(&describe(), include_str!("../nros-runner.toml")).unwrap();
    }

    #[test]
    fn the_image_runs_under_the_emulator_with_its_passthrough() {
        let p = plan(
            "qemu-user:aarch64",
            r#"{"qemu-user":"/usr/bin/qemu-aarch64-static"}"#,
        );
        let (prog, args) = command_line(&p, Path::new("/b/talker")).unwrap();
        assert_eq!(prog, PathBuf::from("/usr/bin/qemu-aarch64-static"));
        assert_eq!(args, ["/b/talker", "--nros-locator", "tcp/10.0.2.2:7447"]);
    }

    #[test]
    fn a_plan_without_the_emulator_is_refused_naming_the_package() {
        let e = command_line(&plan("qemu-user:aarch64", "{}"), Path::new("/b/talker")).unwrap_err();
        assert!(
            e.contains("qemu-aarch64") && e.contains("qemu-user-static"),
            "{e}"
        );
    }

    #[test]
    fn a_qemu_without_futex_lock_pi_is_refused() {
        let q = Path::new("/usr/bin/qemu-aarch64-static");
        let old = too_old(
            q,
            "qemu-aarch64 version 6.2.0 (Debian 1:6.2+dfsg-2ubuntu6.27)\n",
        )
        .unwrap();
        assert!(
            old.contains("6.2") && old.contains("FUTEX_LOCK_PI") && old.contains("7.2"),
            "{old}"
        );
        assert_eq!(too_old(q, "qemu-aarch64 version 7.2.0\n"), None);
        assert_eq!(too_old(q, "qemu-aarch64 version 11.0.0\n"), None);
        assert_eq!(too_old(q, "something else\n"), None);
    }

    #[test]
    fn another_emulator_kind_is_refused() {
        let e = command_line(
            &plan("qemu:mps2-an385", r#"{"qemu-user":"/q"}"#),
            Path::new("/b/talker"),
        )
        .unwrap_err();
        assert!(e.contains("qemu-user:<arch>"), "{e}");
    }
}
