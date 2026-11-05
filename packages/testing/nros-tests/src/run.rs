//! The harness launches a target image the way a user does: `nros run --plan`
//! (phase-528 W5, RFC-0118).
//!
//! A test configures the image as it always has — a [`Command`] naming the
//! built executable, its arguments, its environment and its working directory
//! — and hands it to [`host_image`]. That writes a RunPlan for the image into a
//! private directory and returns the command that executes it,
//! `nros run --plan <file>`. The console is the verb's stdout, and the verdict
//! is the verb's exit code (RFC-0118 D2).
//!
//! What stays with the harness, and why:
//!
//! * **Port leases, ROS domain ids, DDS isolation.** They are the TEST's
//!   decisions. They reach the image as environment and arguments, which the
//!   plan carries (`passthrough`) or the `nros run` process passes on (its
//!   environment is the runner's, and the runner's is the image's). So
//!   `dds_isolation::apply_to_command` applies to the returned command
//!   unchanged.
//! * **Fixture freshness.** The fixture resolvers decided the artifact is
//!   current before the test got its path; the plan names that artifact, and
//!   `--plan` resolves and builds nothing (no compilation inside tests).
//!
//! The verb adds nothing of its own: `--router none`, because a test that
//! needs a router starts the one it leased.
//!
//! Each launch gets its own directory, used as the verb's workspace, so the
//! run directory (`<ws>/build/nros/run/<image>/`, where the verb writes its
//! copy of the plan and the console log) is never shared between two cells
//! that run the same image at once. The directory lives as long as the
//! returned [`HostRun`].

use std::{
    path::{Path, PathBuf},
    process::Command,
};

use crate::{TestError, TestResult};

/// Stop an `nros run` process the way the harness stops a direct child: at
/// once, with SIGKILL.
///
/// The verb owns a process group of its own (the runner and the image), and a
/// SIGKILL to the verb's group alone would leave that group running: the
/// runner dies with the verb (`PR_SET_PDEATHSIG`), the image it spawned does
/// not. So the groups under the verb are killed first, then the verb.
///
/// A verb with nothing under it yet (stopped in the instant before it started
/// its runner) is asked instead: SIGTERM, which it turns into a teardown, then
/// SIGKILL after [`crate::process::KILL_GRACE`]. That is also the path off
/// Linux, where the descendants cannot be listed.
pub fn stop(handle: &mut std::process::Child) {
    #[cfg(target_os = "linux")]
    {
        let verb = handle.id();
        // SAFETY: getpgid reads, kill signals; both on processes this
        // harness started (the verb's descendants).
        let verb_group = unsafe { libc::getpgid(verb as libc::pid_t) };
        let groups: std::collections::BTreeSet<libc::pid_t> = descendants(verb)
            .into_iter()
            .map(|p| unsafe { libc::getpgid(p as libc::pid_t) })
            .filter(|g| *g > 0 && *g != verb_group)
            .collect();
        if !groups.is_empty() {
            for g in groups {
                unsafe { libc::kill(-g, libc::SIGKILL) };
            }
            crate::process::kill_process_group(handle);
            return;
        }
    }
    #[cfg(unix)]
    crate::process::graceful_kill_process_group_within(handle, crate::process::KILL_GRACE);
    #[cfg(not(unix))]
    crate::process::kill_process_group(handle);
}

/// Every live descendant of `pid` (not `pid` itself), from
/// `/proc/<pid>/task/*/children`. Linux only; empty elsewhere.
#[must_use]
pub fn descendants(pid: u32) -> Vec<u32> {
    let mut out = Vec::new();
    #[cfg(target_os = "linux")]
    {
        let mut stack = vec![pid];
        while let Some(p) = stack.pop() {
            if p != pid {
                out.push(p);
            }
            let Ok(tasks) = std::fs::read_dir(format!("/proc/{p}/task")) else {
                continue;
            };
            for t in tasks.flatten() {
                if let Ok(kids) = std::fs::read_to_string(t.path().join("children")) {
                    stack.extend(
                        kids.split_whitespace()
                            .filter_map(|k| k.parse::<u32>().ok()),
                    );
                }
            }
        }
    }
    #[cfg(not(target_os = "linux"))]
    let _ = pid;
    out
}

/// The pid of the image an `nros run` process (`verb_pid`) is running: the
/// descendant (verb -> runner -> image) whose executable is `exe`.
///
/// For a reader of the IMAGE's own process state (`/proc/<pid>/mem`, the
/// native_sim heap gate): the harness's child is the verb now, so its pid
/// names the wrong process. `None` when no such descendant is alive, and
/// always off Linux.
#[must_use]
pub fn image_pid(verb_pid: u32, exe: &Path) -> Option<u32> {
    let want = exe.canonicalize().ok()?;
    descendants(verb_pid).into_iter().find(|p| {
        std::fs::read_link(format!("/proc/{p}/exe")).ok().as_deref() == Some(want.as_path())
    })
}

/// Plan `args` that run a C image line-buffered, `stdbuf -oL -eL <image>`: a C
/// image's stdout is a pipe here, so without it a marker sits in a 4 KiB buffer
/// past the test's deadline.
pub const LINE_BUFFERED: &[(&str, &str)] = &[("prefix", "stdbuf -oL -eL")];

/// The runner package and name for a host process (RFC-0119 D8).
pub const NATIVE_RUNNER_PACKAGE: &str = "nros_runner_native";
/// The runner name the plan states for a host process.
pub const NATIVE_RUNNER: &str = "native";

/// An image launch through `nros run --plan`: the command to spawn and the
/// directory holding its plan, which must outlive the process.
#[derive(Debug)]
pub struct HostRun {
    /// `nros run --plan <file> …`, with the image's environment and working
    /// directory. Spawn it like any other command.
    pub command: Command,
    /// The image command as the test configured it, rendered, for failure
    /// messages: the plan file is gone once the test ends.
    pub image_line: String,
    dir: tempfile::TempDir,
}

impl HostRun {
    /// Move the command out to spawn it; the plan directory stays with `self`.
    pub fn take_command(&mut self) -> Command {
        std::mem::replace(&mut self.command, Command::new("true"))
    }

    /// The plan file the command executes.
    #[must_use]
    pub fn plan_path(&self) -> PathBuf {
        self.dir.path().join("plan.json")
    }
}

/// The plan for a host image: what `nros run --dry-run --json` would print for
/// it, written by the harness instead of resolved from a workspace.
fn host_plan(exe: &Path, passthrough: &[String], plan_args: &[(&str, &str)]) -> serde_json::Value {
    let image = exe
        .file_name()
        .map_or_else(|| "image".to_string(), |n| n.to_string_lossy().into_owned());
    let mut args = serde_json::Map::new();
    for (k, v) in plan_args {
        args.insert((*k).to_string(), (*v).into());
    }
    serde_json::json!({
        "protocol": 1,
        "image": image,
        "artifacts": { "elf": exe },
        "verb": "run",
        "runner": { "package": NATIVE_RUNNER_PACKAGE, "name": NATIVE_RUNNER },
        "target": { "host": true },
        "console": { "kind": "stdio" },
        "args": args,
        "passthrough": passthrough,
        "provenance": { "runner": "nros-tests" },
    })
}

/// Rewrite a host image's command into `nros run --plan`.
///
/// `image` is the command the test would have spawned: the program is the
/// built executable, its arguments go into the plan's `passthrough`, and its
/// environment and working directory move onto the returned command.
/// `plan_args` are the plan's `args` (RFC-0118 D5), which the native runner
/// reads: `seed`, a Zephyr `native_sim` image's `--seed`, and `prefix`, a
/// command the image runs under (`stdbuf -oL -eL`), exactly what
/// `nros run --prefix` states.
///
/// # Errors
///
/// The `nros` CLI is not built, or the plan directory cannot be written.
pub fn host_image(image: &Command, plan_args: &[(&str, &str)]) -> TestResult<HostRun> {
    let nros = crate::nros_cli_bin_path().ok_or_else(|| {
        TestError::ProcessFailed(
            "the `nros` CLI is not built, so no image can be run through `nros run --plan` \
             (run `just setup-cli`)"
                .into(),
        )
    })?;
    let program = PathBuf::from(image.get_program());
    let exe = std::path::absolute(&program).unwrap_or(program);
    let passthrough: Vec<String> = image
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();

    let dir = tempfile::Builder::new().prefix("nros-run-").tempdir()?;
    let plan = dir.path().join("plan.json");
    let text = serde_json::to_string_pretty(&host_plan(&exe, &passthrough, plan_args))
        .map_err(|e| TestError::ProcessFailed(format!("serializing the run plan: {e}")))?;
    std::fs::write(&plan, text)?;

    let mut command = Command::new(nros);
    command
        .arg("run")
        .arg("--plan")
        .arg(&plan)
        .arg("--workspace")
        .arg(dir.path())
        .arg("--nano-ros-path")
        .arg(crate::project_root())
        .args(["--router", "none"]);
    for (k, v) in image.get_envs() {
        match v {
            Some(v) => command.env(k, v),
            None => command.env_remove(k),
        };
    }
    if let Some(cwd) = image.get_current_dir() {
        command.current_dir(cwd);
    }
    Ok(HostRun {
        command,
        image_line: crate::qemu::render_command(image),
        dir,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_plan_names_the_native_runner_and_carries_the_arguments() {
        let p = host_plan(
            Path::new("/b/talker"),
            &["--tcp".into(), "127.0.0.1:7447".into()],
            &[],
        );
        assert_eq!(p["runner"]["name"], NATIVE_RUNNER);
        assert_eq!(p["runner"]["package"], NATIVE_RUNNER_PACKAGE);
        assert_eq!(p["target"]["host"], true);
        assert_eq!(p["artifacts"]["elf"], "/b/talker");
        assert_eq!(p["image"], "talker");
        assert_eq!(p["passthrough"][1], "127.0.0.1:7447");
        assert!(p["args"].get("seed").is_none());
        let s = host_plan(
            Path::new("/native-sim-image/zephyr.exe"),
            &[],
            &[("seed", "42")],
        );
        assert_eq!(s["args"]["seed"], "42");
    }

    /// The image under `nros run` is found by walking the verb's descendants;
    /// a shell standing in for verb and runner shows the walk reaches a
    /// grandchild and matches it by executable.
    #[cfg(target_os = "linux")]
    #[test]
    fn the_image_pid_is_the_descendant_running_the_image() {
        let sleep = std::path::PathBuf::from("/bin/sleep")
            .canonicalize()
            .unwrap();
        let mut verb = Command::new("sh")
            .args(["-c", "sh -c 'sleep 30; true' & wait"])
            .spawn()
            .unwrap();
        let mut found = None;
        for _ in 0..100 {
            found = image_pid(verb.id(), &sleep);
            if found.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let pid = found.expect("the grandchild sleep is found");
        assert_ne!(pid, verb.id());
        assert_eq!(
            std::fs::read_link(format!("/proc/{pid}/exe")).unwrap(),
            sleep
        );
        // SAFETY: signals the process this test spawned.
        unsafe { libc::kill(pid as libc::pid_t, libc::SIGKILL) };
        let _ = verb.kill();
        let _ = verb.wait();
        assert_eq!(image_pid(verb.id(), &sleep), None);
    }

    /// `stop` takes down a group the verb started under it, which a SIGKILL to
    /// the verb's own group misses: a shell that `setsid`s a child stands in
    /// for `nros run` and its runner's group.
    #[cfg(target_os = "linux")]
    #[test]
    fn stop_kills_the_group_under_the_verb() {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "setsid sleep 30 & wait"]);
        crate::process::set_new_process_group(&mut cmd);
        let mut verb = cmd.spawn().unwrap();
        let sleep = std::path::PathBuf::from("/bin/sleep")
            .canonicalize()
            .unwrap();
        let mut child = None;
        for _ in 0..100 {
            child = image_pid(verb.id(), &sleep);
            if child.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let child = child.expect("the setsid child is found");
        stop(&mut verb);
        let mut gone = false;
        for _ in 0..100 {
            // A zombie still answers kill(0); its state is what tells.
            let stat = std::fs::read_to_string(format!("/proc/{child}/stat")).unwrap_or_default();
            if stat.is_empty() || stat.split_whitespace().nth(2) == Some("Z") {
                gone = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(gone, "the child in its own group outlived stop()");
    }

    #[test]
    fn the_command_keeps_the_image_environment_and_directory() {
        if crate::nros_cli_bin_path().is_none() {
            crate::unmet!("nros CLI not built");
        }
        let mut image = Command::new("/b/talker");
        image
            .arg("-x")
            .env("NROS_LOCATOR", "tcp/127.0.0.1:7447")
            .env_remove("ROS_DOMAIN_ID")
            .current_dir("/tmp");
        let run = host_image(&image, &[]).unwrap();
        let envs: Vec<_> = run.command.get_envs().collect();
        assert!(envs.contains(&(
            std::ffi::OsStr::new("NROS_LOCATOR"),
            Some(std::ffi::OsStr::new("tcp/127.0.0.1:7447"))
        )));
        assert!(envs.contains(&(std::ffi::OsStr::new("ROS_DOMAIN_ID"), None)));
        assert_eq!(run.command.get_current_dir(), Some(Path::new("/tmp")));
        let args: Vec<_> = run
            .command
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            args[..3],
            ["run", "--plan", &run.plan_path().display().to_string()]
        );
        assert!(args.windows(2).any(|w| w == ["--router", "none"]));
        let plan: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(run.plan_path()).unwrap()).unwrap();
        assert_eq!(plan["passthrough"][0], "-x");
    }
}
