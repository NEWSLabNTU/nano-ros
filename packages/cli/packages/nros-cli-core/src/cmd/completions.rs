//! `nros completions <shell>` — Phase 111.A.13.

use clap::Args as ClapArgs;
use eyre::{Result, eyre};

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// Shell to emit completion script for
    #[arg(value_parser = ["bash", "zsh", "fish", "powershell"])]
    pub shell: String,
}

pub fn run(_args: Args) -> Result<()> {
    Err(eyre!(
        "`nros completions` is not implemented yet (Phase 111.A.13). \
         Wiring requires the binary front-end to expose its `clap::Command` \
         tree to `clap_complete::generate`; that lives in `nros-cli`."
    ))
}
