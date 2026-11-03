//! The skeleton every runner executable shares: answer `describe`, read the
//! plan, hand it to the runner's own code, and turn a launched process's exit
//! into the contract's exit code (RFC-0119 D6).
//!
//! ```no_run
//! use nros_extension::{describe::Describe, exit::RunnerExit, plan::Verb, runner};
//!
//! fn main() -> std::process::ExitCode {
//!     let describe = Describe::builder().verb(Verb::Run).need("elf").build();
//!     runner::main(&describe, |_verb, plan| {
//!         let Some(elf) = plan.artifacts.get("elf") else {
//!             return runner::fail("the plan carries no `elf` artifact");
//!         };
//!         runner::exec(std::process::Command::new(elf).args(&plan.passthrough))
//!     })
//! }
//! ```
//!
//! What this module does NOT do is anything nros owns (RFC-0119 D6): it never
//! greps the console, never times out, never starts a router and never makes a
//! process group. A launched process inherits the runner's stdin, stdout and
//! stderr, so the console stream reaches nros unmodified.

use std::process::{Command, ExitCode};

use crate::{
    describe::Describe,
    exit::RunnerExit,
    invocation::Invocation,
    plan::{RunPlan, Verb},
};

/// Prefix of every diagnostic a runner built on this module prints.
pub const DIAGNOSTIC_PREFIX: &str = "nros-runner";

/// Print a diagnostic on stderr and answer [`RunnerExit::RunnerFailed`].
pub fn fail(message: impl std::fmt::Display) -> RunnerExit {
    eprintln!("{DIAGNOSTIC_PREFIX}: {message}");
    RunnerExit::RunnerFailed
}

/// The whole of a runner's `main`.
///
/// `describe` answers `<bin> describe`. For `<bin> <verb> --plan <file>` the
/// plan is read and validated, a verb the runner did not declare is refused,
/// and a plan written for another verb is refused (the command line and the
/// plan must agree); everything else is `handle`'s.
pub fn main(describe: &Describe, handle: impl FnOnce(Verb, &RunPlan) -> RunnerExit) -> ExitCode {
    let invocation = match Invocation::from_env() {
        Ok(i) => i,
        Err(e) => return fail(e).into(),
    };
    dispatch(invocation, describe, handle).into()
}

/// [`main`] for an already parsed invocation (testable without a process).
pub fn dispatch(
    invocation: Invocation,
    describe: &Describe,
    handle: impl FnOnce(Verb, &RunPlan) -> RunnerExit,
) -> RunnerExit {
    let (verb, path) = match invocation {
        Invocation::Describe => {
            describe.print();
            return RunnerExit::Success;
        }
        Invocation::Verb { verb, plan } => (verb, plan),
    };
    if !describe.supports(verb) {
        return fail(format!(
            "this runner does not implement `{verb}` (it implements: {})",
            describe
                .verbs
                .iter()
                .map(|v| v.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    let plan = match RunPlan::read(&path) {
        Ok(p) => p,
        Err(e) => return fail(e),
    };
    if plan.verb != verb {
        return fail(format!(
            "asked to `{verb}` a plan written for `{}`",
            plan.verb
        ));
    }
    handle(verb, &plan)
}

/// Spawn `cmd` with the runner's own stdin, stdout and stderr, wait for it,
/// and map its exit: 0 is success, any other status (a signal included) is
/// the target failing, and a process that could not start is the runner
/// failing.
pub fn exec(cmd: &mut Command) -> RunnerExit {
    match cmd.status() {
        Ok(s) if s.success() => RunnerExit::Success,
        Ok(_) => RunnerExit::TargetFailed,
        Err(e) => fail(format!("could not start {:?}: {e}", cmd.get_program())),
    }
}

/// [`exec`] for a tool whose failure means the runner could not do its job
/// (a flasher, a build-system target that boots nothing when it fails): any
/// non-zero status is [`RunnerExit::RunnerFailed`] (RFC-0118 D2: "could not
/// flash").
pub fn exec_tool(cmd: &mut Command) -> RunnerExit {
    match exec(cmd) {
        RunnerExit::TargetFailed => fail(format!("{:?} failed", cmd.get_program())),
        other => other,
    }
}

/// Split a command template into words before expansion (RFC-0118 D5), so a
/// placeholder's value is one argument even when it holds spaces. Words split
/// on whitespace; single quotes keep everything literal, double quotes keep
/// whitespace and honour `\"` and `\\`. No other shell syntax is read: a
/// template is a command line, not a script.
///
/// # Errors
///
/// An unclosed quote.
pub fn split_words(template: &str) -> Result<Vec<String>, String> {
    let mut words = Vec::new();
    let mut cur = String::new();
    let mut in_word = false;
    let mut chars = template.chars();
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(x) => cur.push(x),
                        None => return Err(format!("unclosed ' in {template:?}")),
                    }
                }
            }
            '"' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') => match chars.next() {
                            Some(x @ ('"' | '\\')) => cur.push(x),
                            Some(x) => {
                                cur.push('\\');
                                cur.push(x);
                            }
                            None => return Err(format!("unclosed \" in {template:?}")),
                        },
                        Some(x) => cur.push(x),
                        None => return Err(format!("unclosed \" in {template:?}")),
                    }
                }
            }
            c if c.is_whitespace() => {
                if in_word {
                    words.push(std::mem::take(&mut cur));
                    in_word = false;
                }
            }
            c => {
                in_word = true;
                cur.push(c);
            }
        }
    }
    if in_word {
        words.push(cur);
    }
    Ok(words)
}

/// Whether a runner's `describe` reply and its package's `nros-runner.toml`
/// state the same `protocol`, `verbs` and `needs` (RFC-0119 D2 and D6 state
/// them twice: the descriptor for discovery, the reply for negotiation). For a
/// runner's own test. The descriptor's keys are read one line each, as the
/// JSON their TOML array and integer forms already are, so this needs no TOML
/// parser.
///
/// # Errors
///
/// A key the descriptor does not state on one line, or a value that differs.
pub fn descriptor_agrees(describe: &Describe, descriptor: &str) -> Result<(), String> {
    let value = |key: &str| -> Result<serde_json::Value, String> {
        let line = descriptor
            .lines()
            .find_map(|l| {
                let (k, v) = l.split_once('=')?;
                (k.trim() == key).then_some(v)
            })
            .ok_or_else(|| format!("the descriptor states no one-line `{key}`"))?;
        let v = line.split('#').next().unwrap_or("").trim();
        serde_json::from_str(v).map_err(|e| format!("`{key} = {v}`: {e}"))
    };
    let reply: serde_json::Value =
        serde_json::from_str(&describe.to_json()).map_err(|e| e.to_string())?;
    let needs = value("needs").or_else(|_| Ok::<_, String>(serde_json::json!([])))?;
    for (key, stated) in [
        ("protocol", value("protocol")?),
        ("verbs", value("verbs")?),
        ("needs", needs),
    ] {
        if reply[key] != stated {
            return Err(format!(
                "`{key}`: the descriptor states {stated}, `describe` answers {}",
                reply[key]
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_descriptor_and_a_reply_agree_or_name_the_key() {
        let d = describe();
        assert_eq!(
            descriptor_agrees(
                &d,
                "protocol = 1\nverbs = [\"run\"]  # x\nneeds = [\"elf\"]\n"
            ),
            Ok(())
        );
        let e = descriptor_agrees(
            &d,
            "protocol = 1\nverbs = [\"run\", \"flash\"]\nneeds = [\"elf\"]\n",
        )
        .unwrap_err();
        assert!(e.contains("`verbs`"), "{e}");
        let no_needs = Describe::builder().verb(Verb::Run).build();
        assert_eq!(
            descriptor_agrees(&no_needs, "protocol = 1\nverbs = [\"run\"]\n"),
            Ok(())
        );
    }

    fn describe() -> Describe {
        Describe::builder().verb(Verb::Run).need("elf").build()
    }

    fn plan_file(verb: &str) -> (tempdir::Dir, std::path::PathBuf) {
        let dir = tempdir::Dir::new();
        let path = dir.0.join("plan.json");
        std::fs::write(
            &path,
            format!(
                r#"{{"image":"i","verb":"{verb}","runner":{{"package":"p","name":"n"}},
                "target":{{"host":true}},"console":{{"kind":"stdio"}}}}"#
            ),
        )
        .unwrap();
        (dir, path)
    }

    mod tempdir {
        pub struct Dir(pub std::path::PathBuf);
        impl Dir {
            pub fn new() -> Self {
                use std::sync::atomic::{AtomicU32, Ordering};
                static N: AtomicU32 = AtomicU32::new(0);
                let p = std::env::temp_dir().join(format!(
                    "nros-extension-runner-{}-{}",
                    std::process::id(),
                    N.fetch_add(1, Ordering::Relaxed)
                ));
                std::fs::create_dir_all(&p).unwrap();
                Dir(p)
            }
        }
        impl Drop for Dir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }

    #[test]
    fn a_verb_reaches_the_handler_with_its_plan() {
        let (_d, path) = plan_file("run");
        let mut seen = None;
        let r = dispatch(
            Invocation::Verb {
                verb: Verb::Run,
                plan: path,
            },
            &describe(),
            |v, p| {
                seen = Some((v, p.image.clone()));
                RunnerExit::Success
            },
        );
        assert_eq!(r, RunnerExit::Success);
        assert_eq!(seen, Some((Verb::Run, "i".to_string())));
    }

    #[test]
    fn an_undeclared_verb_a_mismatched_plan_and_a_bad_file_are_runner_failures() {
        let (_d, flash_plan) = plan_file("flash");
        let never = |_: Verb, _: &RunPlan| -> RunnerExit { panic!("must not be reached") };
        let flash = Invocation::Verb {
            verb: Verb::Flash,
            plan: flash_plan.clone(),
        };
        assert_eq!(
            dispatch(flash, &describe(), never),
            RunnerExit::RunnerFailed
        );
        let both = Describe::builder()
            .verb(Verb::Run)
            .verb(Verb::Flash)
            .build();
        let run = Invocation::Verb {
            verb: Verb::Run,
            plan: flash_plan,
        };
        assert_eq!(dispatch(run, &both, never), RunnerExit::RunnerFailed);
        let missing = Invocation::Verb {
            verb: Verb::Run,
            plan: "/nonexistent/plan.json".into(),
        };
        assert_eq!(
            dispatch(missing, &describe(), never),
            RunnerExit::RunnerFailed
        );
    }

    #[test]
    fn exec_maps_exit_statuses() {
        assert_eq!(exec(&mut Command::new("true")), RunnerExit::Success);
        assert_eq!(exec(&mut Command::new("false")), RunnerExit::TargetFailed);
        assert_eq!(
            exec_tool(&mut Command::new("false")),
            RunnerExit::RunnerFailed
        );
        assert_eq!(
            exec(&mut Command::new("/nonexistent/nros-runner-test-bin")),
            RunnerExit::RunnerFailed
        );
    }

    #[test]
    fn templates_split_into_words_before_expansion() {
        assert_eq!(
            split_words("pyocd flash -t stm32f767zi {hex}").unwrap(),
            ["pyocd", "flash", "-t", "stm32f767zi", "{hex}"]
        );
        assert_eq!(
            split_words(r#"  a 'b c' "d \"e\" \\ \n" '' x"#).unwrap(),
            ["a", "b c", r#"d "e" \ \n"#, "", "x"]
        );
        assert!(split_words("a 'b").is_err());
        assert!(split_words("a \"b").is_err());
        assert!(split_words("").unwrap().is_empty());
    }
}
