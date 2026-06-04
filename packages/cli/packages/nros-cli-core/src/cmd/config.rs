//! `nros config show` / `nros config check` — Phase 111.A.6.
//!
//! v1 surface: parse the project's `config.toml`, surface key sections
//! (zenoh, network, wifi, priority, stack) plus the active Cargo
//! features, and merge the `ROS_DOMAIN_ID` environment override.
//!
//! Kconfig (Zephyr) values + the auto-generated `nros_app_config.h`
//! struct land with Phase 112.D — until then `--zephyr` falls back to
//! a "not yet" message.

use clap::{Args as ClapArgs, Subcommand};
use eyre::{Result, WrapErr, eyre};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Subcommand)]
pub enum Args {
    /// Print the resolved configuration (config.toml merged with env
    /// overrides + active Cargo features)
    Show(ShowArgs),
    /// Validate config.toml syntactically and warn on missing common
    /// keys (zenoh.locator, zenoh.domain_id, wifi.{ssid,password})
    Check(CheckArgs),
}

#[derive(Debug, ClapArgs)]
pub struct ShowArgs {
    /// Path to config.toml (default: ./config.toml)
    #[arg(long, default_value = "config.toml")]
    pub config: PathBuf,
}

#[derive(Debug, ClapArgs)]
pub struct CheckArgs {
    /// Path to config.toml (default: ./config.toml)
    #[arg(long, default_value = "config.toml")]
    pub config: PathBuf,
}

pub fn run(args: Args) -> Result<()> {
    match args {
        Args::Show(args) => show(args),
        Args::Check(args) => check(args),
    }
}

fn show(args: ShowArgs) -> Result<()> {
    let cfg = load(&args.config)?;
    println!("# config.toml ({})", args.config.display());
    println!("{}", toml::to_string_pretty(&cfg)?);

    if let Ok(domain_id) = std::env::var("ROS_DOMAIN_ID") {
        println!("# Environment override: ROS_DOMAIN_ID = {domain_id}");
    }
    Ok(())
}

fn check(args: CheckArgs) -> Result<()> {
    let cfg = load(&args.config)?;
    let mut warnings: Vec<String> = Vec::new();

    let zenoh = cfg.get("zenoh").and_then(|v| v.as_table());
    match zenoh {
        Some(t) => {
            if !t.contains_key("locator") {
                warnings.push("zenoh.locator missing".into());
            }
            if !t.contains_key("domain_id") {
                warnings.push("zenoh.domain_id missing (defaults to 0)".into());
            }
        }
        None => warnings.push("[zenoh] section missing".into()),
    }

    if warnings.is_empty() {
        println!("✓ {} OK", args.config.display());
        Ok(())
    } else {
        for w in &warnings {
            eprintln!("warning: {w}");
        }
        Err(eyre!(
            "{} has {} warning(s)",
            args.config.display(),
            warnings.len()
        ))
    }
}

fn load(path: &Path) -> Result<toml::Value> {
    let raw =
        fs::read_to_string(path).wrap_err_with(|| format!("failed to read {}", path.display()))?;
    toml::from_str::<toml::Value>(&raw)
        .wrap_err_with(|| format!("invalid TOML in {}", path.display()))
}
