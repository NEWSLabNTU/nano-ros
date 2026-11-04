//! `nros router` — the zenoh router tools (RFC-0119 D8, phase-527 W5).
//!
//! A command package: `nros router <sub> …` executes this binary with the
//! arguments after `router` unchanged. `nros ws router-config …`, the
//! built-in this replaces, is a hidden alias that executes it as
//! `config …`, so both spellings print the same bytes.

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};
use eyre::{Result, bail};

#[derive(Debug, Parser)]
#[command(
    name = "nros router",
    bin_name = "nros router",
    about = "The zenoh router tools: the configuration a bringup's router runs."
)]
struct Cli {
    #[command(subcommand)]
    cmd: Sub,
}

#[derive(Debug, Subcommand)]
enum Sub {
    /// Print the router `downsampling` rule `nros configure` wrote beside a model.
    ///
    /// Ready to deploy:
    /// `ZENOH_CONFIG_OVERRIDE="$(nros router config --model <model>)"`.
    ///
    /// Reads the artifact rather than re-lowering, so the router runs exactly
    /// the rule configure wrote. Fails when there is none: no subscriber
    /// declares an arrival curve, no image links zenoh, or the model was never
    /// configured.
    // phase-516 W1b, RFC-0074 enforcement point 1.
    Config(ConfigArgs),
}

#[derive(Debug, clap::Args)]
struct ConfigArgs {
    /// The resolved SystemModel (`build/nros/models/<bringup>/<stem>.yaml`);
    /// the rule is the `<stem>.router.json5` beside it.
    #[arg(long, value_name = "PATH")]
    model: PathBuf,
    /// `override` (default): the `ZENOH_CONFIG_OVERRIDE` entry
    /// `rmw_zenohd` reads. `json5`: the artifact itself, for a config file.
    #[arg(long, value_enum, default_value_t = Format::Override)]
    format: Format,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Format {
    Override,
    Json5,
}

fn main() -> Result<()> {
    match Cli::parse().cmd {
        Sub::Config(a) => config(a),
    }
}

/// `nros router config --model <model.yaml>` — phase-516 W1b's deploy door.
// The messages keep the `router-config:` prefix the built-in printed, so the
// old spelling's output is unchanged (phase-527 W5 acceptance).
fn config(args: ConfigArgs) -> Result<()> {
    let path = nros_orchestration_ir::router_rule::router_rule_path_for_model(&args.model);
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => bail!(
            "router-config: no router rule at {} — no subscriber of this model declares an \
             `arrival_curve`, no image links zenoh, or `nros configure` has not run since it did",
            path.display()
        ),
        Err(e) => bail!("router-config: read {}: {e}", path.display()),
    };
    match args.format {
        Format::Json5 => print!("{text}"),
        Format::Override => println!(
            "{}",
            nros_orchestration_ir::router_rule::config_override_from_artifact(&text)
                .map_err(|e| eyre::eyre!("router-config: {}: {e}", path.display()))?
        ),
    }
    Ok(())
}
