//! `nros-runner-command`: the `command` runner (RFC-0119 D8), `run` and
//! `flash`.
//!
//! Executes a command template that a board or image table states in place of
//! a method (RFC-0118 D5):
//!
//! ```toml
//! [image.lab.flash]
//! command = "pyocd flash -t stm32f767zi {hex}"
//! ```
//!
//! The template reaches the plan as `args.command`. It is split into words
//! FIRST and each word expanded after, so a placeholder's value stays one
//! argument however many spaces it holds. Placeholders come only from the
//! plan, and an unknown one is refused, never passed through. Arguments after
//! `--` follow the template's words. No shell runs: a template is one command
//! line, not a script.
//!
//! Exit: for `run`, the command IS the target, so its failure is the target's
//! (1). For `flash`, a failing command is a flash that did not happen, which
//! is the runner's failure (2, RFC-0118 D2).

use std::process::{Command, ExitCode};

use nros_extension::{
    describe::Describe,
    exit::RunnerExit,
    plan::{RunPlan, Verb},
    runner,
};

fn describe() -> Describe {
    Describe::builder()
        .verb(Verb::Run)
        .verb(Verb::Flash)
        .build()
}

fn main() -> ExitCode {
    runner::main(&describe(), run)
}

/// The expanded argument vector, program first.
fn argv(plan: &RunPlan) -> Result<Vec<String>, String> {
    let template = plan
        .args
        .get("command")
        .ok_or("the plan carries no `args.command` template to execute")?;
    let words = runner::split_words(template).map_err(|e| format!("`command`: {e}"))?;
    if words.is_empty() {
        return Err("the `command` template is empty".to_string());
    }
    let mut out = words
        .iter()
        .map(|w| {
            plan.expand(w)
                .map_err(|e| format!("`command = {template:?}`: {e}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    out.extend(plan.passthrough.iter().cloned());
    Ok(out)
}

fn run(verb: Verb, plan: &RunPlan) -> RunnerExit {
    let argv = match argv(plan) {
        Ok(a) => a,
        Err(e) => return runner::fail(e),
    };
    let mut cmd = Command::new(&argv[0]);
    cmd.args(&argv[1..]);
    match verb {
        Verb::Flash => runner::exec_tool(&mut cmd),
        _ => runner::exec(&mut cmd),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(command: &str) -> RunPlan {
        let mut p = RunPlan::from_json(
            r#"{"image":"i","verb":"flash","runner":{"package":"nros_runner_command","name":"command"},
            "target":{"flasher":"command"},"console":{"kind":"serial","port":"/dev/ttyACM0"},
            "artifacts":{"hex":"/my build/app.hex"},
            "tools":{"pyocd":"/sdk/pyocd/bin/pyocd"},"passthrough":["--verbose"]}"#,
        )
        .unwrap();
        p.args.insert("command".into(), command.into());
        p
    }

    #[test]
    fn describe_agrees_with_the_descriptor() {
        runner::descriptor_agrees(&describe(), include_str!("../nros-runner.toml")).unwrap();
    }

    #[test]
    fn a_placeholder_is_one_argument_and_passthrough_follows() {
        assert_eq!(
            argv(&plan(
                "{tool:pyocd} flash -t stm32f767zi {hex} --port={port}"
            ))
            .unwrap(),
            [
                "/sdk/pyocd/bin/pyocd",
                "flash",
                "-t",
                "stm32f767zi",
                "/my build/app.hex",
                "--port=/dev/ttyACM0",
                "--verbose"
            ]
        );
    }

    #[test]
    fn unknown_missing_and_empty_templates_are_refused() {
        assert!(
            argv(&plan("tool {home}"))
                .unwrap_err()
                .contains("unknown placeholder")
        );
        assert!(argv(&plan("tool {uf2}")).unwrap_err().contains("no value"));
        assert!(argv(&plan("   ")).is_err());
        assert!(argv(&plan("tool 'x")).is_err());
        let mut p = plan("x");
        p.args.remove("command");
        assert!(argv(&p).is_err());
    }

    #[test]
    fn a_failing_flash_is_the_runners_failure_and_a_failing_run_the_targets() {
        let mut p = plan("false");
        p.passthrough.clear();
        assert_eq!(run(Verb::Flash, &p), RunnerExit::RunnerFailed);
        p.verb = Verb::Run;
        assert_eq!(run(Verb::Run, &p), RunnerExit::TargetFailed);
        let mut ok = plan("true");
        ok.passthrough.clear();
        assert_eq!(run(Verb::Flash, &ok), RunnerExit::Success);
    }
}
