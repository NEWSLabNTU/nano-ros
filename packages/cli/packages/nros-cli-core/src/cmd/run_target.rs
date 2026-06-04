//! `nros run` — build → flash → monitor. Phase 111.A.10.
//!
//! v1 surface:
//!   - Cargo + native target → `cargo run` (build + exec the default bin)
//!   - Cargo + ESP32 (`xtensa-esp32*-none-elf` / `riscv32imc*` esp32c*) →
//!     `espflash flash --monitor`
//!   - Cmake / Zephyr → not yet wired; error with a clear pointer
//!
//! Multi-target QEMU launch (mps2-an385, nuttx-virt, threadx-…) goes
//! through the existing `just <plat> run` recipes for now; v2 of this
//! verb absorbs them.

use clap::Args as ClapArgs;
use eyre::{Result, WrapErr, eyre};
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// Path to the project root (default: cwd)
    #[arg(long)]
    pub project: Option<PathBuf>,

    /// Named target/env (matches a `[env.<name>]` section in the
    /// project config). Optional when the project has only one target.
    #[arg(long)]
    pub env: Option<String>,

    /// Trailing arguments forwarded verbatim
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub passthrough: Vec<String>,
}

pub fn run(args: Args) -> Result<()> {
    let root = match args.project {
        Some(p) => p,
        None => std::env::current_dir()?,
    };

    if root.join("prj.conf").is_file() {
        return Err(eyre!(
            "Zephyr `nros run` is not yet wired up. Use `west flash` after `nros build`."
        ));
    }

    let cargo_toml = root.join("Cargo.toml");
    if !cargo_toml.is_file() {
        return Err(eyre!(
            "no Cargo.toml at {}; cmake / west run paths are not yet wired",
            root.display()
        ));
    }

    let target = detect_cargo_target(&cargo_toml)?;
    if let Some(t) = target.as_deref()
        && (t.starts_with("xtensa-esp32") || t.starts_with("riscv32imc"))
    {
        return run_esp32(&root, &args.passthrough);
    }

    let mut cmd = Command::new("cargo");
    cmd.arg("run").current_dir(&root);
    cmd.args(&args.passthrough);
    cmd.stdout(Stdio::inherit()).stderr(Stdio::inherit());
    let status = cmd
        .status()
        .wrap_err_with(|| format!("failed to invoke `cargo run` in {}", root.display()))?;
    if !status.success() {
        return Err(eyre!(
            "`cargo run` failed (exit {})",
            status.code().unwrap_or(-1)
        ));
    }
    Ok(())
}

fn run_esp32(root: &Path, passthrough: &[String]) -> Result<()> {
    let mut cmd = Command::new("espflash");
    cmd.arg("flash")
        .arg("--monitor")
        .current_dir(root)
        .args(passthrough)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    let status = cmd
        .status()
        .wrap_err("failed to invoke `espflash` (install with `cargo install espflash`)")?;
    if !status.success() {
        return Err(eyre!(
            "`espflash flash --monitor` failed (exit {})",
            status.code().unwrap_or(-1)
        ));
    }
    Ok(())
}

/// Best-effort target detection from `.cargo/config.toml`. Returns
/// `Some(target_triple)` when an explicit `[build].target` is set.
fn detect_cargo_target(cargo_toml: &Path) -> Result<Option<String>> {
    let cargo_dir = cargo_toml
        .parent()
        .ok_or_else(|| eyre!("invalid Cargo.toml path"))?;
    let config = cargo_dir.join(".cargo").join("config.toml");
    if !config.is_file() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(&config)
        .wrap_err_with(|| format!("failed to read {}", config.display()))?;
    let doc: toml::Value = toml::from_str(&raw)?;
    Ok(doc
        .get("build")
        .and_then(|b| b.get("target"))
        .and_then(|t| t.as_str())
        .map(|s| s.to_string()))
}
