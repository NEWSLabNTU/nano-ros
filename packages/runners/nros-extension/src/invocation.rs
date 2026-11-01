//! A runner's own command line (RFC-0119 D6).
//!
//! ```text
//! <bin> describe
//! <bin> run|flash|console --plan <plan.json>
//! ```

use std::{fmt, path::PathBuf};

use crate::plan::Verb;

/// What nros asked the runner for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Invocation {
    /// Print the `describe` reply.
    Describe,
    /// Execute a verb against a plan file.
    Verb {
        /// The verb.
        verb: Verb,
        /// The `--plan` file.
        plan: PathBuf,
    },
}

/// The command line is not one the contract defines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageError(pub String);

impl fmt::Display for UsageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}\nusage: <runner> describe\n       <runner> run|flash|console --plan <plan.json>",
            self.0
        )
    }
}

impl std::error::Error for UsageError {}

impl Invocation {
    /// Parse the process's own arguments.
    ///
    /// # Errors
    ///
    /// A [`UsageError`].
    pub fn from_env() -> Result<Invocation, UsageError> {
        Invocation::parse(
            std::env::args_os()
                .skip(1)
                .map(|a| a.to_string_lossy().into_owned()),
        )
    }

    /// Parse arguments, excluding the program name.
    ///
    /// # Errors
    ///
    /// A [`UsageError`].
    pub fn parse<I, S>(args: I) -> Result<Invocation, UsageError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let args: Vec<String> = args.into_iter().map(Into::into).collect();
        let Some((first, rest)) = args.split_first() else {
            return Err(UsageError("no verb given".into()));
        };
        if first == "describe" {
            if !rest.is_empty() {
                return Err(UsageError(format!(
                    "`describe` takes no arguments, got {rest:?}"
                )));
            }
            return Ok(Invocation::Describe);
        }
        let verb =
            Verb::parse(first).ok_or_else(|| UsageError(format!("unknown verb {first:?}")))?;
        let plan = match rest {
            [flag, path] if flag == "--plan" => PathBuf::from(path),
            [one] if one.starts_with("--plan=") => PathBuf::from(&one["--plan=".len()..]),
            _ => {
                return Err(UsageError(format!(
                    "`{verb}` takes exactly `--plan <plan.json>`, got {rest:?}"
                )));
            }
        };
        Ok(Invocation::Verb { verb, plan })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_contract_forms() {
        assert_eq!(Invocation::parse(["describe"]), Ok(Invocation::Describe));
        assert_eq!(
            Invocation::parse(["run", "--plan", "p.json"]),
            Ok(Invocation::Verb {
                verb: Verb::Run,
                plan: "p.json".into()
            })
        );
        assert_eq!(
            Invocation::parse(["console", "--plan=p.json"]),
            Ok(Invocation::Verb {
                verb: Verb::Console,
                plan: "p.json".into()
            })
        );
    }

    #[test]
    fn refuses_anything_else() {
        assert!(Invocation::parse(Vec::<String>::new()).is_err());
        assert!(Invocation::parse(["describe", "x"]).is_err());
        assert!(Invocation::parse(["debug", "--plan", "p"]).is_err());
        assert!(Invocation::parse(["run"]).is_err());
        assert!(Invocation::parse(["run", "--plan", "p", "extra"]).is_err());
    }
}
