//! `nros completions [<shell>]` — the line that turns on completion
//! (RFC-0133 D7, phase-569 W4).
//!
//! Completion is dynamic: the shell asks `nros` itself on every `<Tab>`
//! (`COMPLETE=<shell> nros …`), so names come from the SDK root and the
//! workspace as they are now. The line printed here registers that once.
//! `--static` prints a generated script instead, which completes verbs and
//! flags but no names; the front-end answers it, because generating it needs
//! the whole command tree.

use clap::Args as ClapArgs;
use eyre::{Result, eyre};

use crate::cmd::complete::{SHELLS, render, shell_from_env};

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// The shell. Default: the one `$SHELL` names.
    #[arg(value_parser = SHELLS)]
    pub shell: Option<String>,
    /// Print a generated script instead of the registration line: verbs and
    /// flags complete, names do not.
    #[arg(long = "static")]
    pub static_script: bool,
}

/// The shell to complete in: the one asked for, else `$SHELL`'s.
pub fn shell(args: &Args) -> Result<String> {
    if let Some(s) = &args.shell {
        return Ok(s.clone());
    }
    shell_from_env(std::env::var("SHELL").ok().as_deref()).ok_or_else(|| {
        eyre!(
            "`$SHELL` names no shell `nros` completes in; name one: \
             `nros completions <{}>`",
            SHELLS.join("|")
        )
    })
}

pub fn run(args: Args) -> Result<()> {
    print!("{}", render(&shell(&args)?)?);
    Ok(())
}
