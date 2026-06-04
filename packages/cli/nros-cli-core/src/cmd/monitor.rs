//! `nros monitor` — Phase 111.A.11.
//!
//! v1: ESP32 → `espflash monitor`. Other targets surface a pointer to
//! the per-platform tooling. Decoded panic prints (defmt-print, semihosting
//! addr2line) land alongside Phase 88 (`nros-log`) when that ships.

use clap::Args as ClapArgs;
use eyre::{Result, WrapErr, eyre};
use std::process::{Command, Stdio};

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// Named target/env (matches `nros run --env`)
    #[arg(long)]
    pub env: Option<String>,

    /// Trailing arguments forwarded verbatim
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub passthrough: Vec<String>,
}

pub fn run(args: Args) -> Result<()> {
    // v1: assume ESP32 monitor is the desired path. Other platforms
    // print their own panic decoders directly via the QEMU stdout
    // stream (semihosting) — for those, `just <plat> run` already
    // tails what's needed.
    let mut cmd = Command::new("espflash");
    cmd.arg("monitor")
        .args(&args.passthrough)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    let status = cmd
        .status()
        .wrap_err("failed to invoke `espflash` (install with `cargo install espflash`)")?;
    if !status.success() {
        return Err(eyre!(
            "`espflash monitor` failed (exit {})",
            status.code().unwrap_or(-1)
        ));
    }
    Ok(())
}
