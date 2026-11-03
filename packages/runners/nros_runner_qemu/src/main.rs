//! `nros-runner-qemu`: the `qemu` runner (RFC-0119 D8), `run` only.
//!
//! Boots the plan's `elf` under QEMU. The argument line is rendered from the
//! plan's QEMU `args` by `nros_qemu_ladder::plan_line`, the one renderer
//! `nros build`'s printed next step uses too, so the line a person reads is
//! the line this runner executes.
//!
//! The `qemu-system-<arch>` executable is the plan's: `tools` maps its name
//! to an absolute path that nros resolved through the QEMU ladder (the
//! `QEMU_SYSTEM_<ARCH>` override, the checkout's pinned prefix, the SDK
//! store). This runner never searches `PATH`: a distribution QEMU lacks the
//! patches the store's carries, and running one nobody chose is how a test
//! measures the wrong emulator.

use std::process::{Command, ExitCode};

use nros_extension::{
    describe::Describe,
    exit::RunnerExit,
    plan::{RunPlan, Verb},
    runner,
};

fn describe() -> Describe {
    Describe::builder().verb(Verb::Run).need("elf").build()
}

fn main() -> ExitCode {
    runner::main(&describe(), |_verb, plan| run(plan))
}

/// The executable and its argument vector.
fn command_line(plan: &RunPlan) -> Result<(std::path::PathBuf, Vec<String>), String> {
    match plan.target.emulator.as_deref() {
        Some(e) if e.starts_with("qemu:") => {}
        other => {
            return Err(format!(
                "the plan's target is {other:?}; the qemu runner boots `qemu:<machine>`"
            ));
        }
    }
    let elf = plan
        .artifacts
        .get("elf")
        .ok_or("the plan carries no `elf` artifact to boot")?;
    let line = nros_qemu_ladder::plan_line(&plan.args)?;
    let exe = plan.tools.get(&line.exe).cloned().ok_or_else(|| {
        format!(
            "the plan's tools name no `{}`; nros resolves QEMU through the SDK store \
             (`nros setup --tool qemu`, or ${}), and this runner never takes one from PATH",
            line.exe,
            nros_qemu_ladder::env_var(&line.exe)
        )
    })?;
    let mut argv = line.argv(&elf.display().to_string());
    argv.extend(plan.passthrough.iter().cloned());
    Ok((exe, argv))
}

fn run(plan: &RunPlan) -> RunnerExit {
    match command_line(plan) {
        Ok((exe, argv)) => runner::exec(Command::new(exe).args(argv)),
        Err(e) => runner::fail(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(args: &str, tools: &str) -> RunPlan {
        RunPlan::from_json(&format!(
            r#"{{"image":"i","verb":"run","runner":{{"package":"nros_runner_qemu","name":"qemu"}},
            "target":{{"emulator":"qemu:mps2-an385"}},"console":{{"kind":"semihosting"}},
            "artifacts":{{"elf":"/b/app"}},"args":{args},"tools":{tools}}}"#
        ))
        .unwrap()
    }

    #[test]
    fn describe_agrees_with_the_descriptor() {
        runner::descriptor_agrees(&describe(), include_str!("../nros-runner.toml")).unwrap();
    }

    #[test]
    fn the_line_is_the_ladders_and_the_executable_the_plans() {
        let args = r#"{"system":"arm","cpu":"cortex-m3","machine":"mps2-an385",
            "icount":"shift=auto","semihosting":"enable=on,target=native","nic":"user,model=lan9118"}"#;
        let p = plan(
            args,
            r#"{"qemu-system-arm":"/sdk/qemu/bin/qemu-system-arm"}"#,
        );
        let (exe, argv) = command_line(&p).unwrap();
        assert_eq!(
            exe,
            std::path::PathBuf::from("/sdk/qemu/bin/qemu-system-arm")
        );
        let line = nros_qemu_ladder::plan_line(&p.args).unwrap();
        assert_eq!(
            argv,
            line.argv("/b/app"),
            "the printed line is the executed one"
        );
        assert_eq!(argv.last().map(String::as_str), Some("/b/app"));
    }

    #[test]
    fn a_fixed_ladder_board_renders_too() {
        let p = plan(
            r#"{"board":"threadx-riscv64","mac":"52:54:00:12:34:56"}"#,
            r#"{"qemu-system-riscv64":"/sdk/qemu/bin/qemu-system-riscv64"}"#,
        );
        let (_, argv) = command_line(&p).unwrap();
        assert!(argv.iter().any(|a| a == "/b/app"), "{argv:?}");
    }

    #[test]
    fn no_tool_no_path_fallback() {
        let p = plan(r#"{"system":"arm","machine":"mps2-an385"}"#, "{}");
        let e = command_line(&p).unwrap_err();
        assert!(
            e.contains("qemu-system-arm") && e.contains("QEMU_SYSTEM_ARM"),
            "{e}"
        );
    }

    #[test]
    fn args_that_name_no_line_are_refused() {
        let p = plan(r#"{"cpu":"cortex-m3"}"#, "{}");
        assert!(command_line(&p).is_err());
    }
}
