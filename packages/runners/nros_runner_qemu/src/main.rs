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
//! start the router on the other end. QEMU starts paused (`-S`) and resumes
//! over QMP [`SERIAL_SETTLE`] after the link is reported: a pty with nothing
//! on the other end drops what the guest writes, so a guest that boots first
//! sends its session handshake into nothing, and the router then reads the
//! guest's retry as a protocol error.

use std::{
    io::{BufRead, BufReader, Read, Write},
    os::unix::net::UnixStream,
    process::{Command, ExitCode, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use nros_extension::{
    console,
    describe::Describe,
    exit::RunnerExit,
    plan::{ConsoleKind, LinkKind, RunPlan, Verb},
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
        // `-serial` replaces `-nographic`'s stdio mux, which would otherwise
        // leave QEMU's monitor prompt on the console: no monitor at all.
        line.before_kernel.splice(
            at..at,
            ["-serial", "pty", "-monitor", "none"].map(String::from),
        );
    }
    let mut argv = line.argv(&elf.display().to_string());
    argv.extend(plan.passthrough.iter().cloned());
    Ok((exe, argv))
}

/// Whether QEMU's stderr is part of the board's console. Under semihosting
/// the guest writes to both host streams (its `stderr` — where the board's log
/// bridge prints, `[INFO] … Publishing: …` included — lands on QEMU's), and
/// the runner protocol grades only stdout, so on a semihosting console both
/// streams are the console. Other consoles (QEMU's stdio mux) keep QEMU's
/// stderr as the runner's diagnostics.
fn console_includes_stderr(plan: &RunPlan) -> bool {
    plan.console.kind == ConsoleKind::Semihosting
}

/// This process's stdout, as a child's stdio handle.
fn stdout_as_stdio() -> std::io::Result<Stdio> {
    use std::os::fd::AsFd;
    Ok(Stdio::from(std::io::stdout().as_fd().try_clone_to_owned()?))
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

/// Copy `from` to `to` unmodified, and report the first pty line seen by
/// either stream (`reported` is shared between the two copies).
fn copy_and_scan(mut from: impl Read, mut to: impl Write, reported: &AtomicBool) {
    let mut buf = [0u8; 4096];
    let mut line: Vec<u8> = Vec::new();
    loop {
        let n = match from.read(&mut buf) {
            Ok(0) | Err(_) => return,
            Ok(n) => n,
        };
        let _ = to.write_all(&buf[..n]).and_then(|()| to.flush());
        if reported.load(Ordering::SeqCst) {
            continue;
        }
        for &b in &buf[..n] {
            if b != b'\n' {
                line.push(b);
                continue;
            }
            let text = String::from_utf8_lossy(&line).into_owned();
            line.clear();
            if let Some(pty) = redirected_pty(&text)
                && !reported.swap(true, Ordering::SeqCst)
            {
                console::report_link(pty);
                break;
            }
        }
    }
}

/// How long a serial guest stays paused after its pty is reported: the time
/// nros has to start the router on the other end (the test harness waits the
/// same before booting a serial guest).
const SERIAL_SETTLE: Duration = Duration::from_secs(2);

/// Resume a QEMU started with `-S`, over its QMP socket.
fn qmp_cont(path: &std::path::Path) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut sock = loop {
        match UnixStream::connect(path) {
            Ok(s) => break s,
            Err(e) if Instant::now() >= deadline => {
                return Err(format!("QMP socket {}: {e}", path.display()));
            }
            Err(_) => std::thread::sleep(Duration::from_millis(50)),
        }
    };
    let _ = sock.set_read_timeout(Some(Duration::from_secs(5)));
    let mut reader = BufReader::new(sock.try_clone().map_err(|e| e.to_string())?);
    let mut line = String::new();
    // The greeting, then one reply per command.
    for cmd in [
        "",
        "{\"execute\":\"qmp_capabilities\"}",
        "{\"execute\":\"cont\"}",
    ] {
        if !cmd.is_empty() {
            writeln!(sock, "{cmd}").map_err(|e| format!("QMP write: {e}"))?;
        }
        line.clear();
        reader
            .read_line(&mut line)
            .map_err(|e| format!("QMP read: {e}"))?;
        if line.contains("\"error\"") {
            return Err(format!("QMP refused `{cmd}`: {}", line.trim()));
        }
    }
    Ok(())
}

fn run(plan: &RunPlan) -> RunnerExit {
    let (exe, argv) = match command_line(plan) {
        Ok(c) => c,
        Err(e) => return runner::fail(e),
    };
    let merge = console_includes_stderr(plan);
    if !serial_link(plan) {
        let mut cmd = Command::new(exe);
        cmd.args(argv);
        if merge {
            match stdout_as_stdio() {
                Ok(s) => {
                    cmd.stderr(s);
                }
                Err(e) => return runner::fail(format!("duplicating stdout: {e}")),
            }
        }
        return runner::exec(&mut cmd);
    }
    // Serial: QEMU names the pty it allocated (`char device redirected to
    // ...`) on stdout or stderr depending on its version. Both streams are
    // copied through byte for byte, and the first such line is reported.
    let qmp = std::env::temp_dir().join(format!("nros-runner-qemu-{}.qmp", std::process::id()));
    let _ = std::fs::remove_file(&qmp);
    let mut child = match Command::new(&exe)
        .args(&argv)
        .arg("-S")
        .arg("-qmp")
        .arg(format!("unix:{},server=on,wait=off", qmp.display()))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return runner::fail(format!("could not start {}: {e}", exe.display())),
    };
    let reported = Arc::new(AtomicBool::new(false));
    let out = child.stdout.take().expect("piped");
    let err = child.stderr.take().expect("piped");
    let r1 = Arc::clone(&reported);
    let t1 = std::thread::spawn(move || copy_and_scan(out, std::io::stdout(), &r1));
    let r2 = Arc::clone(&reported);
    let t2 = std::thread::spawn(move || {
        let to: Box<dyn Write> = if merge {
            Box::new(std::io::stdout())
        } else {
            Box::new(std::io::stderr())
        };
        copy_and_scan(err, to, &r2);
    });
    // Hold the guest until the link is reported and the router had time to
    // open the other end; a guest QEMU cannot resume is the runner failing.
    let deadline = Instant::now() + Duration::from_secs(10);
    while !reported.load(Ordering::SeqCst) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    std::thread::sleep(SERIAL_SETTLE);
    let resumed = qmp_cont(&qmp);
    let _ = std::fs::remove_file(&qmp);
    if let Err(e) = resumed {
        let _ = child.kill();
        let _ = child.wait();
        return runner::fail(format!("resuming the paused guest: {e}"));
    }
    let status = child.wait();
    let _ = t1.join();
    let _ = t2.join();
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
        assert_eq!(
            &argv[k - 4..k],
            ["-serial", "pty", "-monitor", "none"],
            "{argv:?}"
        );
        assert_eq!(
            redirected_pty("char device redirected to /dev/pts/7 (label serial0)"),
            Some("/dev/pts/7")
        );
        assert_eq!(redirected_pty("qemu: something else"), None);
    }

    /// A semihosting guest's log lines reach QEMU's stderr, and the grader
    /// reads stdout only: both streams are that console.
    #[test]
    fn a_semihosting_console_includes_qemus_stderr() {
        let mut p = plan(r#"{"system":"arm","machine":"mps2-an385"}"#, "{}");
        assert!(console_includes_stderr(&p));
        p.console.kind = ConsoleKind::Stdio;
        assert!(!console_includes_stderr(&p));
    }

    #[test]
    fn args_that_name_no_line_are_refused() {
        let p = plan(r#"{"cpu":"cortex-m3"}"#, "{}");
        assert!(command_line(&p).is_err());
    }
}
