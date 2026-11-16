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

use std::{path::PathBuf, process::ExitCode};

use nros_extension::{
    describe::Describe,
    exit::RunnerExit,
    plan::{RunPlan, Verb},
    runner,
};
use nros_runner_lifetime as lifetime;
use nros_runner_west::{
    build_dir, no_passthrough, run_argv, run_targets, target_for, west_command,
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

/// What the runner hands west: the `west build -d <dir> -t run_armfvp`
/// argument vector and the model's directory for `ARMFVP_BIN_PATH`.
///
/// Nothing here names a model PARAMETER, and that is the design (issue 2629):
/// the board's and the image's `-C` parameters are in the `run_armfvp`
/// command Zephyr's `armfvp.cmake` wrote at configure time, which is what
/// `west build -t run` runs too. A flag added here would make `nros run`
/// boot a different model from the one `west build -t run` boots.
fn prepare(plan: &RunPlan) -> Result<(Vec<String>, Option<PathBuf>), String> {
    no_passthrough(plan)?;
    let m = model(plan)?;
    let dir = build_dir(plan)?;
    let target = target_for(ZEPHYR_EMULATOR, run_targets(&dir).as_deref())?;
    Ok((run_argv(&dir, &target), model_dir(plan, m)))
}

fn run(plan: &RunPlan) -> RunnerExit {
    match prepare(plan) {
        Ok((argv, fvp_dir)) => {
            let mut cmd = west_command(plan);
            cmd.args(argv);
            if let Some(d) = fvp_dir {
                cmd.env("ARMFVP_BIN_PATH", d);
            }
            // west, then ninja, then the model: a chain (issue 2307).
            lifetime::exec_chain(&mut cmd, runner::exec)
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

    /// Issue 2629 — `nros run` boots the model `west build -t run` boots: the
    /// runner runs Zephyr's own `run_armfvp` target and passes no model
    /// parameter of its own, so the configure's `ARMFVP_EXTRA_FLAGS` are the
    /// only ones either road gives the model.
    #[test]
    fn the_runner_adds_no_model_flag_of_its_own() {
        let dir = std::env::temp_dir().join(format!("nros-runner-fvp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("build.ninja"),
            "build run_armfvp: CUSTOM_COMMAND
  COMMAND = FVP_BaseR_AEMv8R -C bp.x=1
",
        )
        .unwrap();
        let plan = RunPlan::from_json(&format!(
            r#"{{"image":"i","verb":"run","runner":{{"package":"nros_runner_fvp","name":"fvp"}},
            "target":{{"emulator":"fvp:FVP_BaseR_AEMv8R"}},"console":{{"kind":"stdio"}},
            "build_dir":{:?}}}"#,
            dir.display().to_string()
        ))
        .unwrap();
        let (argv, model_dir) = prepare(&plan).unwrap();
        assert_eq!(
            argv,
            [
                "build".to_string(),
                "-d".to_string(),
                dir.display().to_string(),
                "-t".to_string(),
                "run_armfvp".to_string(),
            ]
        );
        assert!(
            !argv.iter().any(|a| a == "-C" || a.contains("ARMFVP")),
            "{argv:?}"
        );
        assert_eq!(model_dir, None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
