//! The `nros` standalone binary — Phase 111.A.2.
//!
//! Pure clap dispatch shell. All real work lives in `nros-cli-core`.

use clap::{CommandFactory, Parser};
use clap_complete::{Shell, generate};
use eyre::Result;
use nros_cli_core::cmd::Cmd;
use std::io;

#[derive(Parser, Debug)]
#[command(
    name = "nros",
    about = "The nano-ros CLI: scaffold, generate, provision SDKs, plan, check, and inspect.",
    long_about = "nros — command-line tool for nano-ros (a lightweight ROS 2 client for \
                  embedded RTOS).\n\n\
                  Quick start:\n  \
                  nros setup <board>   provision a board's toolchains + sources (board-scoped)\n  \
                  nros new <name>      scaffold a project\n  \
                  nros sync            generate msg bindings + write .cargo patches (Rust)\n  \
                  nros plan            resolve a launch topology\n  \
                  nros check           validate a plan or workspace\n  \
                  nros doctor          check SDK paths / toolchains / env\n\n\
                  Run `nros setup --list` to see available packages.",
    version,
    propagate_version = true
)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    // `completions` is wired here (not in nros-cli-core) because clap_complete
    // needs the binary's `clap::Command` tree, which lives at the
    // front-end. Phase 111.A.13.
    if let Cmd::Completions(args) = &cli.command {
        let shell: Shell = args
            .shell
            .parse()
            .map_err(|e| eyre::eyre!("unsupported shell `{}`: {e}", args.shell))?;
        let mut cmd = Cli::command();
        let bin = cmd.get_name().to_string();
        generate(shell, &mut cmd, bin, &mut io::stdout());
        return Ok(());
    }
    nros_cli_core::run(cli.command)
}
