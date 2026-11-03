//! `nros-runner-west`: the `west` runner (RFC-0119 D8), `run` only.
//!
//! Boots a Zephyr image on the emulator its plan names, through the run
//! target the board's own west build declares: `west build -d <build_dir> -t
//! run_<emulator>`, where `<emulator>` is the kind in the plan's
//! `target.emulator` (`qemu:mps2-an385` boots under `run_qemu`). Zephyr owns
//! the emulator's command line; this runner restates none of it.

use std::process::{Command, ExitCode};

use nros_extension::{
    describe::Describe,
    exit::RunnerExit,
    plan::{RunPlan, Verb},
    runner,
};
use nros_runner_west::{
    build_dir, no_passthrough, run_argv, run_targets, target_for, west_program,
};

fn describe() -> Describe {
    Describe::builder()
        .verb(Verb::Run)
        .need("build_dir")
        .build()
}

fn main() -> ExitCode {
    runner::main(&describe(), |_verb, plan| run(plan))
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
