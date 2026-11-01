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
                  Run `nros setup --list` to see available packages, and \
                  `nros help --all` for the build-system verbs under `nros dev`.",
    version,
    propagate_version = true,
    // `--codegen-version` is answerable with no subcommand, so the subcommand
    // stops being clap-required; this keeps bare `nros` printing help.
    arg_required_else_help = true
)]
struct Cli {
    /// Print the CODEGEN VERSION this binary emits, and exit (phase-429 W2).
    ///
    /// The compatibility token between this binary's emitted code and the
    /// nano-ros runtime that compiles it — deliberately not a release version.
    /// Visible, unlike the `codegen-fingerprint` / `source-stamp` seams: the
    /// guard's own refusal message tells a user to compare this number against
    /// the runtime's accepted range, so the door it names has to be one they
    /// can find in `--help`.
    #[arg(long)]
    codegen_version: bool,

    #[command(subcommand)]
    command: Option<Cmd>,
}

fn main() -> Result<()> {
    // phase-440 W7 / RFC-0095 D8 — the launcher, BEFORE clap.
    //
    // Not an optimisation and not a style choice: D8 requires this binary to
    // keep working when it is OLDER than the toolchain it launches, and an
    // older binary does not know a newer one's flags. Anything parsed here
    // could be parsed wrong on behalf of a binary that would have got it right,
    // so nothing is parsed here — the decision reads `nros-toolchain.toml` and
    // this process's own path, and `argv` is forwarded byte for byte.
    //
    // Returns when this process should carry on (no pin, the pin names it, a
    // contributor's checkout, a non-store build); otherwise it never returns.
    // RFC-0103 D6 — one store-root variable. Refused before the dispatch,
    // which is the first thing that resolves the store.
    if let Some(e) = nros_cli_core::orchestration::store::retired_in_env() {
        eyre::bail!(e);
    }
    nros_cli_core::orchestration::dispatch::redispatch()?;
    // phase-517 W10 — `nros setup` installs rustup into `~/.cargo` without
    // touching the user's PATH, so every `cargo` this process (or a `cmake` it
    // drives) spawns must find it there. One place, for every spawn site;
    // before clap and before any command spawns a thread.
    nros_cli_core::orchestration::rust_toolchain::expose_cargo_bin();
    // issue 2197 — `$NROS_OFFLINE` reaches every cargo this process spawns.
    nros_cli_core::orchestration::rust_toolchain::expose_offline_to_cargo();
    // RFC-0117 D1: `nros dev` is hidden from `nros --help`, and
    // `nros help --all` is where it is listed. clap's `help` subcommand takes
    // no flags, so this is answered before clap parses.
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.first().map(String::as_str) == Some("help") && argv[1..].iter().any(|a| a == "--all") {
        let mut cmd = Cli::command().mut_subcommand("dev", |c| c.hide(false));
        cmd.print_help()?;
        println!();
        cmd.find_subcommand_mut("dev")
            .expect("`nros dev` is a subcommand")
            .clone()
            .bin_name("nros dev")
            .print_help()?;
        return Ok(());
    }
    let cli = Cli::parse();
    if cli.codegen_version {
        println!("{}", nros_cli_core::abi_guard::EMITTED_VERSION);
        return Ok(());
    }
    let Some(command) = cli.command else {
        // `arg_required_else_help` handles the no-args case inside clap, so
        // this is only reachable if a future flag joins `--codegen-version`.
        Cli::command().print_help()?;
        return Ok(());
    };
    // `completions` is wired here (not in nros-cli-core) because clap_complete
    // needs the binary's `clap::Command` tree, which lives at the
    // front-end. Phase 111.A.13.
    if let Cmd::Completions(args) = &command {
        let shell: Shell = args
            .shell
            .parse()
            .map_err(|e| eyre::eyre!("unsupported shell `{}`: {e}", args.shell))?;
        let mut cmd = Cli::command();
        let bin = cmd.get_name().to_string();
        generate(shell, &mut cmd, bin, &mut io::stdout());
        return Ok(());
    }
    nros_cli_core::run(command)
}
