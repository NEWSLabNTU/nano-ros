//! `nros-runner-fvp`: the `fvp` runner (RFC-0119 D8), `run` only.
//!
//! Boots a west-built image on an Arm Fixed Virtual Platform through
//! Zephyr's own FVP run target (`west build -d <build_dir> -t run_armfvp`),
//! which reads the model and its arguments from the board's `board.cmake`.
//!
//! Zephyr resolves the model when the build CONFIGURES and caches it
//! (`ARMFVP_BIN_PATH`). When the plan carries the model as a tool (the
//! plan's `target.emulator` names it, `fvp:<model>`), its directory is handed
//! to west as `ARMFVP_BIN_PATH` too, so a model provisioned after the
//! configure is still found; otherwise the build's cached answer stands.

use std::{
    path::PathBuf,
    process::{Command, ExitCode},
};

use nros_extension::{
    describe::Describe,
    exit::RunnerExit,
    plan::{RunPlan, Verb},
    runner,
};
use nros_runner_west::{
    build_dir, no_passthrough, run_argv, run_targets, target_for, west_program,
};

/// Zephyr's emulator name for an Arm FVP, which Zephyr's run target is named after.
const ZEPHYR_EMULATOR: &str = "armfvp";

fn describe() -> Describe {
    Describe::builder()
        .verb(Verb::Run)
        .need("build_dir")
        .build()
}

fn main() -> ExitCode {
    runner::main(&describe(), |_verb, plan| run(plan))
}

/// The FVP model the plan names (`fvp:FVP_BaseR_AEMv8R` → `FVP_BaseR_AEMv8R`).
fn model(plan: &RunPlan) -> Result<&str, String> {
    match plan
        .target
        .emulator
        .as_deref()
        .and_then(|e| e.split_once(':'))
    {
        Some(("fvp", m)) if !m.is_empty() => Ok(m),
        _ => Err(format!(
            "the plan's target is {:?}; the fvp runner boots `fvp:<model>`",
            plan.target.emulator
        )),
    }
}

/// `ARMFVP_BIN_PATH` for the model, when the plan carries its path.
fn model_dir(plan: &RunPlan, model: &str) -> Option<PathBuf> {
    plan.tools
        .get(model)
        .and_then(|p| p.parent())
        .map(PathBuf::from)
}

fn run(plan: &RunPlan) -> RunnerExit {
    let prepared = (|| {
        no_passthrough(plan)?;
        let m = model(plan)?;
        let dir = build_dir(plan)?;
        let target = target_for(ZEPHYR_EMULATOR, run_targets(&dir).as_deref())?;
        Ok::<_, String>((run_argv(&dir, &target), model_dir(plan, m)))
    })();
    match prepared {
        Ok((argv, fvp_dir)) => {
            let mut cmd = Command::new(west_program(plan));
            cmd.args(argv);
            if let Some(d) = fvp_dir {
                cmd.env("ARMFVP_BIN_PATH", d);
            }
            runner::exec(&mut cmd)
        }
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
    fn the_model_comes_from_the_target_and_its_dir_from_the_tools() {
        let mut plan = RunPlan::from_json(
            r#"{"image":"i","verb":"run","runner":{"package":"nros_runner_fvp","name":"fvp"},
            "target":{"emulator":"fvp:FVP_BaseR_AEMv8R"},"console":{"kind":"stdio"},
            "build_dir":"/b"}"#,
        )
        .unwrap();
        assert_eq!(model(&plan).unwrap(), "FVP_BaseR_AEMv8R");
        assert_eq!(model_dir(&plan, "FVP_BaseR_AEMv8R"), None);
        plan.tools.insert(
            "FVP_BaseR_AEMv8R".into(),
            "/sdk/arm-fvp/models/FVP_BaseR_AEMv8R".into(),
        );
        assert_eq!(
            model_dir(&plan, "FVP_BaseR_AEMv8R"),
            Some(PathBuf::from("/sdk/arm-fvp/models"))
        );
        plan.target.emulator = Some("qemu:virt".into());
        assert!(model(&plan).is_err());
    }
}
