//! `nros-runner-serial`: the `serial` runner (RFC-0119 D8), `console` only.
//!
//! The default console for `nros monitor` and `nros flash --monitor`. It opens
//! the plan's `console.port` raw at `console.baud`, prints what the board
//! sends on stdout unmodified, and sends `args.send` and its stdin to the
//! board with `args.line_ending`. A plan with a `console.command` template
//! runs that tool instead (`picocom -b {baud} {port}`). It runs until nros
//! stops it; a port that goes away is the runner's failure (exit 2).

use std::{
    path::Path,
    process::{Command, ExitCode},
};

use nros_extension::{
    describe::Describe,
    exit::RunnerExit,
    plan::{RunPlan, Verb},
    runner,
};
use nros_runner_serial::{command_argv, open, serve, settings};

fn describe() -> Describe {
    Describe::builder().verb(Verb::Console).need("port").build()
}

fn main() -> ExitCode {
    runner::main(&describe(), |_verb, plan| console(plan))
}

fn console(plan: &RunPlan) -> RunnerExit {
    if let Some(template) = &plan.console.command {
        return match command_argv(plan, template) {
            // The tool IS the console: if it cannot run or fails, the console
            // could not be opened.
            Ok(argv) => runner::exec_tool(Command::new(&argv[0]).args(&argv[1..])),
            Err(e) => runner::fail(e),
        };
    }
    if !plan.passthrough.is_empty() {
        return runner::fail(format!(
            "arguments after `--` ({}) have nowhere to go: the serial console runs no tool. \
             State a `console.command` to attach one that takes them",
            plan.passthrough.join(" ")
        ));
    }
    let s = match settings(plan) {
        Ok(s) => s,
        Err(e) => return runner::fail(e),
    };
    let port = match open(Path::new(&s.port), s.baud) {
        Ok(p) => p,
        Err(e) => return runner::fail(format!("could not open {}: {e}", s.port)),
    };
    match serve(port, &s, std::io::stdin(), std::io::stdout()) {
        Ok(()) => RunnerExit::Success,
        Err(e) => runner::fail(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describe_agrees_with_the_descriptor() {
        runner::descriptor_agrees(&describe(), include_str!("../nros-runner.toml")).unwrap();
    }
}
