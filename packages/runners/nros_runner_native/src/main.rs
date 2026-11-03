//! `nros-runner-native`: the `native` runner (RFC-0119 D8), `run` only.
//!
//! Runs a host image as a process: a Linux or POSIX native build,
//! `threadx-linux`, `freertos-posix`, or Zephyr `native_sim`'s `zephyr.exe`.
//! The process inherits the runner's stdio, so its console reaches nros
//! unmodified.
//!
//! A `native_sim` image gets two things a plain host binary does not:
//!
//! - a `--seed`. Unseeded `native_sim` processes share the test entropy
//!   source, so two of them pick the same GUIDs and ports and each discovers
//!   the other as itself. The seed is the plan's `args.seed` when it states
//!   one (the test harness does, for its pairs), else one derived from the
//!   clock and this process's id, so two runs started together still differ.
//! - arguments after `--` go after `-testargs`, the image's own argv, because
//!   `native_sim`'s option parser aborts on an option it did not register.

use std::{
    path::Path,
    process::{Command, ExitCode},
};

use nros_extension::{
    describe::Describe,
    exit::RunnerExit,
    plan::{RunPlan, Verb},
    runner,
};

/// The file name a Zephyr `native_sim` build gives its host executable.
const NATIVE_SIM_EXE: &str = "zephyr.exe";

fn describe() -> Describe {
    Describe::builder().verb(Verb::Run).need("elf").build()
}

fn main() -> ExitCode {
    runner::main(&describe(), |_verb, plan| run(plan))
}

fn is_native_sim(exe: &Path) -> bool {
    exe.file_name().is_some_and(|n| n == NATIVE_SIM_EXE)
}

/// A seed no other process started at the same moment shares.
fn fresh_seed() -> u32 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    nanos ^ std::process::id().wrapping_mul(2_654_435_761)
}

/// The argument vector after the executable.
fn argv(plan: &RunPlan, exe: &Path, seed: impl FnOnce() -> u32) -> Result<Vec<String>, String> {
    if !is_native_sim(exe) {
        if plan.args.contains_key("seed") {
            return Err(format!(
                "the plan states `args.seed`, which only a Zephyr native_sim image ({NATIVE_SIM_EXE}) \
                 takes; {} is not one",
                exe.display()
            ));
        }
        return Ok(plan.passthrough.clone());
    }
    let seed = match plan.args.get("seed") {
        Some(s) => s
            .parse::<u32>()
            .map_err(|e| format!("`args.seed = {s:?}` is not a 32-bit unsigned number: {e}"))?,
        None => seed(),
    };
    let mut v = vec![format!("--seed={seed}")];
    if !plan.passthrough.is_empty() {
        v.push("-testargs".to_string());
        v.extend(plan.passthrough.iter().cloned());
    }
    Ok(v)
}

fn run(plan: &RunPlan) -> RunnerExit {
    let Some(exe) = plan.artifacts.get("elf") else {
        return runner::fail("the plan carries no `elf` artifact to run");
    };
    if !plan.target.host {
        return runner::fail(
            "the plan's target is not a host process; the native runner runs host images",
        );
    }
    let args = match argv(plan, exe, fresh_seed) {
        Ok(a) => a,
        Err(e) => return runner::fail(e),
    };
    match prefixed(plan, exe, args) {
        Ok((program, args)) => runner::exec(Command::new(program).args(args)),
        Err(e) => runner::fail(e),
    }
}

/// `nros run --prefix <cmd>` (`args.prefix`): run the image under that
/// command (`valgrind -q`, `gdb --args`), split into words as a command
/// template is, never through a shell.
fn prefixed(
    plan: &RunPlan,
    exe: &Path,
    args: Vec<String>,
) -> Result<(String, Vec<String>), String> {
    let mut image = vec![exe.display().to_string()];
    image.extend(args);
    let Some(prefix) = plan.args.get("prefix") else {
        let program = image.remove(0);
        return Ok((program, image));
    };
    let mut words = runner::split_words(prefix)?;
    if words.is_empty() {
        return Err("`args.prefix` is empty".to_string());
    }
    let program = words.remove(0);
    words.extend(image);
    Ok((program, words))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_prefix_wraps_the_image() {
        let mut p = plan("/b/talker");
        let exe = Path::new("/b/talker");
        let (prog, rest) = prefixed(&p, exe, vec!["-x".into()]).unwrap();
        assert_eq!(prog, "/b/talker");
        assert_eq!(rest, ["-x"]);
        p.args.insert("prefix".into(), "valgrind -q".into());
        let (prog, rest) = prefixed(&p, exe, vec!["-x".into()]).unwrap();
        assert_eq!(prog, "valgrind");
        assert_eq!(rest, ["-q", "/b/talker", "-x"]);
    }

    fn plan(elf: &str) -> RunPlan {
        RunPlan::from_json(&format!(
            r#"{{"image":"i","verb":"run","runner":{{"package":"nros_runner_native","name":"native"}},
            "target":{{"host":true}},"console":{{"kind":"stdio"}},"artifacts":{{"elf":"{elf}"}},
            "passthrough":["--nros-locator","tcp/127.0.0.1:7447"]}}"#
        ))
        .unwrap()
    }

    #[test]
    fn describe_agrees_with_the_descriptor() {
        runner::descriptor_agrees(&describe(), include_str!("../nros-runner.toml")).unwrap();
    }

    #[test]
    fn a_host_binary_gets_the_passthrough_unchanged() {
        let p = plan("/b/talker");
        assert_eq!(
            argv(&p, Path::new("/b/talker"), || unreachable!()).unwrap(),
            ["--nros-locator", "tcp/127.0.0.1:7447"]
        );
        let mut p = p;
        p.args.insert("seed".into(), "7".into());
        assert!(argv(&p, Path::new("/b/talker"), || 0).is_err());
    }

    #[test]
    fn native_sim_is_seeded_and_its_arguments_follow_testargs() {
        let mut p = plan("/b/zephyr/zephyr.exe");
        let exe = Path::new("/b/zephyr/zephyr.exe");
        assert_eq!(
            argv(&p, exe, || 42).unwrap(),
            [
                "--seed=42",
                "-testargs",
                "--nros-locator",
                "tcp/127.0.0.1:7447"
            ]
        );
        p.args.insert("seed".into(), "1234".into());
        assert_eq!(argv(&p, exe, || unreachable!()).unwrap()[0], "--seed=1234");
        p.args.insert("seed".into(), "-1".into());
        assert!(argv(&p, exe, || 0).is_err());
        p.args.remove("seed");
        p.passthrough.clear();
        assert_eq!(argv(&p, exe, || 5).unwrap(), ["--seed=5"]);
    }
}
