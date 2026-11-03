//! `nros-runner-west`: the `west` runner (RFC-0119 D8), `run` and `flash`.
//!
//! - `run` boots a Zephyr image on the emulator its plan names, through the
//!   run target the board's own west build declares: `west build -d
//!   <build_dir> -t run_<emulator>`, where `<emulator>` is the kind in the
//!   plan's `target.emulator` (`qemu:mps2-an385` boots under `run_qemu`).
//! - `flash` programs the board through Zephyr's own runner: `west flash -d
//!   <build_dir> -r <runner> [-- <args>]`. The runner is the plan's
//!   `args.runner` (`nros flash --runner jlink`), else the board's
//!   `flash-runner` from the build's `zephyr/runners.yaml`; arguments after
//!   `--` go to that runner unchanged. A failed flash is exit 2.
//!
//! Zephyr owns the emulator's command line and the flash runner's; this
//! runner restates neither.

use std::process::{Command, ExitCode};

use nros_extension::{
    describe::Describe,
    exit::RunnerExit,
    plan::{RunPlan, Verb},
    runner,
};
use nros_runner_west::{
    build_dir, flash_argv, flash_runner, flash_runners, no_passthrough, run_argv, run_targets,
    target_for, west_program,
};

fn describe() -> Describe {
    Describe::builder()
        .verb(Verb::Run)
        .verb(Verb::Flash)
        .need("build_dir")
        .build()
}

fn main() -> ExitCode {
    runner::main(&describe(), |verb, plan| match verb {
        Verb::Flash => flash(plan),
        _ => run(plan),
    })
}

/// Zephyr's emulator name for the plan's emulator kind.
fn emulator_kind(plan: &RunPlan) -> Result<&str, String> {
    let emu = plan.target.emulator.as_deref().ok_or_else(|| {
        "the plan's target is not an emulator; the west runner boots emulated boards".to_string()
    })?;
    Ok(emu.split_once(':').map_or(emu, |(k, _)| k))
}

fn run(plan: &RunPlan) -> RunnerExit {
    let prepared = (|| {
        no_passthrough(plan)?;
        let dir = build_dir(plan)?;
        let kind = emulator_kind(plan)?;
        let target = target_for(kind, run_targets(&dir).as_deref())?;
        Ok::<_, String>(run_argv(&dir, &target))
    })();
    match prepared {
        Ok(argv) => runner::exec(Command::new(west_program(plan)).args(argv)),
        Err(e) => runner::fail(e),
    }
}

fn flash(plan: &RunPlan) -> RunnerExit {
    let prepared = (|| {
        let dir = build_dir(plan)?;
        let runners = flash_runners(&dir)?;
        let r = flash_runner(plan.args.get("runner").map(String::as_str), &runners)?;
        Ok::<_, String>(flash_argv(&dir, &r, &plan.passthrough))
    })();
    match prepared {
        Ok(argv) => runner::exec_tool(Command::new(west_program(plan)).args(argv)),
        Err(e) => runner::fail(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describe_agrees_with_the_descriptor() {
        runner::descriptor_agrees(&describe(), include_str!("../nros-runner.toml")).unwrap();
    }

    #[test]
    fn the_emulator_kind_is_the_prefix() {
        let mut plan = RunPlan::from_json(
            r#"{"image":"i","verb":"run","runner":{"package":"nros_runner_west","name":"west"},
            "target":{"emulator":"renode:nucleo_f767zi"},"console":{"kind":"serial"}}"#,
        )
        .unwrap();
        assert_eq!(emulator_kind(&plan).unwrap(), "renode");
        plan.target.emulator = None;
        plan.target.host = true;
        assert!(emulator_kind(&plan).is_err());
    }
}
