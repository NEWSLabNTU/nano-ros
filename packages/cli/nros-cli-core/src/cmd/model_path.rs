//! `nros model-path` — phase-330 W7.c: map the INPUT coordinates of a system
//! (`bringup dir`, launch file, launch args) to the resolved SystemModel path.
//!
//! This is the cmake bridge for the input-addressed entry spelling
//! (`nano_ros_entry(LAUNCH …)`): the mapping rule lives ONCE in
//! `nros_orchestration_ir::model_location` (shared with `nros::main!`'s
//! `launch =` arm), and cmake reaches it through this verb instead of
//! re-implementing the rule in cmake (the second-spelling drift class).
//!
//! Prints the model path a consumer should READ (the W3.b search ladder:
//! `$NROS_MODEL_DIR` → `$OUT_DIR/nros` → the committed copy) — the first
//! existing candidate, else the committed location so the error a caller
//! surfaces names the file a user can create.

use std::path::PathBuf;

use clap::Parser;
use eyre::{Result, WrapErr, bail};

#[derive(Debug, Parser)]
pub struct Args {
    /// The bringup package DIRECTORY (holds `system.toml` + `launch/`).
    #[arg(long = "bringup-dir", value_name = "DIR")]
    pub bringup_dir: PathBuf,

    /// Launch file name relative to `<bringup>/launch/` (default: the
    /// bringup's `[system] default_launch`, conventionally
    /// `system.launch.xml`).
    #[arg(long = "launch", value_name = "FILE")]
    pub launch: Option<String>,

    /// Launch argument binding `key=value` (repeatable). Arg-bound variants
    /// must match a `[[model]]` declaration in `system.toml` (phase-330
    /// W4.0 derive-plus-declare).
    #[arg(long = "arg", value_name = "K=V")]
    pub args: Vec<String>,
}

pub fn run(args: Args) -> Result<()> {
    let bringup_dir = args
        .bringup_dir
        .canonicalize()
        .wrap_err_with(|| format!("bringup dir `{}`", args.bringup_dir.display()))?;
    let mut launch_args: Vec<(String, String)> = Vec::new();
    for kv in &args.args {
        let Some((k, v)) = kv.split_once('=') else {
            bail!("--arg takes `key=value`, got `{kv}`");
        };
        launch_args.push((k.to_string(), v.to_string()));
    }
    let model_rel = nros_orchestration_ir::model_location::launch_to_model_rel(
        &bringup_dir,
        args.launch.as_deref(),
        &launch_args,
    )
    .map_err(|e| eyre::eyre!(e))?;
    let path =
        nros_orchestration_ir::model_location::resolve_model_path(&bringup_dir, &model_rel);
    println!("{}", path.display());
    Ok(())
}
