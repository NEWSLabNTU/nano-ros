//! `nros run`, `flash`, `monitor`, `bench` and `debug` — reserved verbs
//! (RFC-0117 D2, RFC-0118).
//!
//! The names are taken now so nothing else claims them (a command package
//! cannot provide a reserved verb, RFC-0119), and so a user who types one is
//! told what to do instead. Each refuses until its implementation lands
//! (run: phase-528; flash, monitor, bench: phase-529).

use clap::Args as ClapArgs;
use eyre::Result;

/// The reserved verbs, in help order. `check-provider-announcements` and the
/// command dispatch read this so a command package cannot claim one.
pub const VERBS: &[&str] = &["run", "flash", "monitor", "bench", "debug"];

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// Accepted and ignored until the verb is available.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true, hide = true)]
    pub args: Vec<String>,
}

/// The refusal a reserved verb prints.
pub fn refusal(verb: &str) -> String {
    format!(
        "`nros {verb}` is reserved and is not available in this version of nros.\n  \
         After a successful build, `nros build` prints the command that runs or \
         flashes what it built; use that command for now."
    )
}

pub fn run(verb: &str, _args: Args) -> Result<()> {
    eyre::bail!("{}", refusal(verb))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmd::dev::tests::Cli;
    use clap::{CommandFactory, Parser};

    #[test]
    fn every_reserved_verb_parses_and_refuses() {
        let root = Cli::command();
        for verb in VERBS {
            assert!(
                root.find_subcommand(verb).is_some(),
                "`nros {verb}` is not in the tree"
            );
            let cli = Cli::try_parse_from(["nros", verb, "--anything", "x"])
                .unwrap_or_else(|e| panic!("`nros {verb}` does not parse: {e}"));
            let err = crate::run(cli.cmd).expect_err("a reserved verb refuses");
            assert!(
                err.to_string()
                    .contains(&format!("`nros {verb}` is reserved")),
                "{err}"
            );
        }
    }
}
