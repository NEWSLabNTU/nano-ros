//! `nros-runner-qemu`: the `qemu` runner (RFC-0119 D8), `run` only.
//!
//! Boots the plan's `elf` under QEMU. The argument line is rendered from the
//! plan's QEMU `args` by `nros_qemu_ladder::plan_line`, the one renderer
//! `nros build`'s printed next step uses too, so the line a person reads is
//! the line this runner executes.
//!
//! The `qemu-system-<arch>` executable is the plan's: `tools` maps its name
//! to an absolute path that nros resolved through the QEMU ladder (the
//! `QEMU_SYSTEM_<ARCH>` override, the checkout's pinned prefix, the SDK
//! store). This runner never searches `PATH`: a distribution QEMU lacks the
//! patches the store's carries, and running one nobody chose is how a test
//! measures the wrong emulator.
//!
//! A plan whose link is serial gets the first UART on a host pty
//! (`-serial pty`), and the runner reports that pty on stderr as
//! `nros-runner: link <pty>` (RFC-0119 D6) once QEMU names it, so nros can
//! start the router on the other end.

use std::{
    io::{BufRead, BufReader, Write},
    process::{Command, ExitCode, Stdio},
};

use nros_extension::{
    console,
    describe::Describe,
    exit::RunnerExit,
    plan::{LinkKind, RunPlan, Verb},
    runner,
};

fn describe() -> Describe {
    Describe::builder().verb(Verb::Run).need("elf").build()
}

fn main() -> ExitCode {
    runner::main(&describe(), |_verb, plan| run(plan))
}

/// The executable and its argument vector.
fn command_line(plan: &RunPlan) -> Result<(std::path::PathBuf, Vec<String>), String> {
    match plan.target.emulator.as_deref() {
        Some(e) if e.starts_with("qemu:") => {}
        other => {
            return Err(format!(
                "the plan's target is {other:?}; the qemu runner boots `qemu:<machine>`"
            ));
        }
    }
    let elf = plan
        .artifacts
        .get("elf")
        .ok_or("the plan carries no `elf` artifact to boot")?;
    let line = nros_qemu_ladder::plan_line(&plan.args)?;
    let exe = plan.tools.get(&line.exe).cloned().ok_or_else(|| {
        format!(
            "the plan's tools name no `{}`; nros resolves QEMU through the SDK store \
             (`nros setup --tool qemu`, or ${}), and this runner never takes one from PATH",
            line.exe,
            nros_qemu_ladder::env_var(&line.exe)
        )
    })?;
    let mut line = line;
    if serial_link(plan) {
        // Ahead of `-kernel`, the last word before the image.
        let at = line.before_kernel.len().saturating_sub(1);
        line.before_kernel
            .splice(at..at, ["-serial".to_string(), "pty".to_string()]);
    }
    let mut argv = line.argv(&elf.display().to_string());
    argv.extend(plan.passthrough.iter().cloned());
    Ok((exe, argv))
}

fn serial_link(plan: &RunPlan) -> bool {
    plan.link
        .as_ref()
        .is_some_and(|l| l.kind == LinkKind::Serial)
}

/// The pty in QEMU's `char device redirected to /dev/pts/N (label ...)`.
fn redirected_pty(line: &str) -> Option<&str> {
    let rest = line.split_once("char device redirected to ")?.1;
    rest.split_whitespace()
        .next()
        .filter(|p| p.starts_with('/'))
}

fn run(plan: &RunPlan) -> RunnerExit {
    let (exe, argv) = match command_line(plan) {
        Ok(c) => c,
        Err(e) => return runner::fail(e),
    };
    if !serial_link(plan) {
        return runner::exec(Command::new(exe).args(argv));
    }
    // Serial: watch QEMU's stderr for the pty it allocated; forward every line.
    let mut child = match Command::new(&exe)
        .args(&argv)
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return runner::fail(format!("could not start {}: {e}", exe.display())),
    };
    let stderr = child.stderr.take().expect("piped");
    let watcher = std::thread::spawn(move || {
        let mut reported = false;
        let mut err = std::io::stderr();
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            let _ = writeln!(err, "{line}");
            if !reported && let Some(pty) = redirected_pty(&line) {
                console::report_link(pty);
                reported = true;
            }
        }
    });
    let status = child.wait();
    let _ = watcher.join();
    match status {
        Ok(s) if s.success() => RunnerExit::Success,
        Ok(_) => RunnerExit::TargetFailed,
        Err(e) => runner::fail(format!("waiting for {}: {e}", exe.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(args: &str, tools: &str) -> RunPlan {
        RunPlan::from_json(&format!(
            r#"{{"image":"i","verb":"run","runner":{{"package":"nros_runner_qemu","name":"qemu"}},
            "target":{{"emulator":"qemu:mps2-an385"}},"console":{{"kind":"semihosting"}},
            "artifacts":{{"elf":"/b/app"}},"args":{args},"tools":{tools}}}"#
        ))
        .unwrap()
    }

    #[test]
    fn describe_agrees_with_the_descriptor() {
        runner::descriptor_agrees(&describe(), include_str!("../nros-runner.toml")).unwrap();
    }

    #[test]
    fn the_line_is_the_ladders_and_the_executable_the_plans() {
        let args = r#"{"system":"arm","cpu":"cortex-m3","machine":"mps2-an385",
            "icount":"shift=auto","semihosting":"enable=on,target=native","nic":"user,model=lan9118"}"#;
        let p = plan(
            args,
            r#"{"qemu-system-arm":"/sdk/qemu/bin/qemu-system-arm"}"#,
        );
        let (exe, argv) = command_line(&p).unwrap();
        assert_eq!(
            exe,
            std::path::PathBuf::from("/sdk/qemu/bin/qemu-system-arm")
        );
        let line = nros_qemu_ladder::plan_line(&p.args).unwrap();
        assert_eq!(
            argv,
            line.argv("/b/app"),
            "the printed line is the executed one"
        );
        assert_eq!(argv.last().map(String::as_str), Some("/b/app"));
    }

    #[test]
    fn a_fixed_ladder_board_renders_too() {
        let p = plan(
            r#"{"board":"threadx-riscv64","mac":"52:54:00:12:34:56"}"#,
            r#"{"qemu-system-riscv64":"/sdk/qemu/bin/qemu-system-riscv64"}"#,
        );
        let (_, argv) = command_line(&p).unwrap();
        assert!(argv.iter().any(|a| a == "/b/app"), "{argv:?}");
    }

    #[test]
    fn no_tool_no_path_fallback() {
        let p = plan(r#"{"system":"arm","machine":"mps2-an385"}"#, "{}");
        let e = command_line(&p).unwrap_err();
        assert!(
            e.contains("qemu-system-arm") && e.contains("QEMU_SYSTEM_ARM"),
            "{e}"
        );
    }

    #[test]
    fn a_serial_link_puts_the_uart_on_a_pty_and_reports_it() {
        let args = r#"{"system":"arm","cpu":"cortex-m3","machine":"mps2-an385"}"#;
        let mut p = plan(args, r#"{"qemu-system-arm":"/q"}"#);
        p.link = Some(nros_extension::plan::Link {
            kind: LinkKind::Serial,
            locator: "serial/UART_0#baudrate=115200".into(),
            rmw: None,
        });
        let (_, argv) = command_line(&p).unwrap();
        let k = argv.iter().position(|a| a == "-kernel").unwrap();
        assert_eq!(&argv[k - 2..k], ["-serial", "pty"], "{argv:?}");
        assert_eq!(
            redirected_pty("char device redirected to /dev/pts/7 (label serial0)"),
            Some("/dev/pts/7")
        );
        assert_eq!(redirected_pty("qemu: something else"), None);
    }

    #[test]
    fn args_that_name_no_line_are_refused() {
        let p = plan(r#"{"cpu":"cortex-m3"}"#, "{}");
        assert!(command_line(&p).is_err());
    }
}
