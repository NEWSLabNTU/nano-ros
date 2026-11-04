//! `nros-runner-renode`: the `renode` runner (RFC-0119 D8), `run` only.
//!
//! Boots the plan's `elf` in the Renode machine its target names
//! (`renode:<machine>`, Renode's `platforms/boards/<machine>.repl`). The
//! board's wiring arrives in the plan's `args`, which nros fills from the
//! descriptor's `[board.run.renode]`: `link` is the `sysbus` UART that carries
//! the image's serial link, and `console` the UART its console prints on.
//!
//! Why not Zephyr's own `run_renode` target (phase-524 W4b, measured): Zephyr
//! 3.7's `nucleo_f767zi` declares none (its `board.cmake` includes openocd and
//! jlink only, so the build has no `run_renode` edge and the `west` runner
//! refused the plan), and a board that does declare one boots Zephyr's own
//! `.resc`, which puts no UART on a host pty for a router to open.
//!
//! What it does, the pattern phase-524 W1 measured
//! (docs/roadmap/data/phase-524-w1-renode-nucleo.log):
//!
//! * Renode starts PAUSED on a script this runner writes: the machine, the
//!   link UART on a host pty Renode creates itself (`CreateUartPtyTerminal`),
//!   the console UART into a file, the ELF loaded.
//! * Once the pty exists the runner reports it on stderr as
//!   `nros-runner: link <pty>` (RFC-0119 D6), so nros starts the router on the
//!   other end, waits [`SERIAL_SETTLE`], and starts the simulation over
//!   Renode's monitor port: a pty with nothing on the other end drops what the
//!   guest writes, which would lose the session handshake.
//! * The console file is copied to stdout unmodified, which is what nros
//!   grades `--expect` over; Renode's own output is diagnostics, on stderr.
//!
//! Renode is the plan's tool: `tools.renode`, which nros resolves through
//! `nros locate tool renode` (`$NROS_RENODE`, then the store at the pinned
//! `[tool.renode]`). This runner never searches `PATH`. Renode writes
//! `~/.config/renode` whatever `--config` says (measured in W1), so its `HOME`
//! is the runner's scratch directory; the portable build resolves
//! `@platforms/...` against its own directory, which is its working directory.
//!
//! Renode runs in the runner's process group, so the group nros stops on
//! `--expect`, `--timeout` or Ctrl-C includes it.

use std::{
    fs::File,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, ExitCode, Stdio},
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
    plan::{LinkKind, RunPlan, Verb},
    runner,
};

fn describe() -> Describe {
    Describe::builder().verb(Verb::Run).need("elf").build()
}

fn main() -> ExitCode {
    runner::main(&describe(), |_verb, plan| run(plan))
}

/// How long the guest stays paused after its pty is reported: the time nros
/// has to start the router on the other end (the qemu runner waits the same).
const SERIAL_SETTLE: Duration = Duration::from_secs(2);

/// How long Renode gets to create the link pty (it loads the platform and the
/// ELF first; about 5 s measured on the W1 spike's host).
const PTY_WITHIN: Duration = Duration::from_secs(60);

/// The plan's facts this runner reads, checked up front.
#[derive(Debug, PartialEq, Eq)]
struct Twin {
    renode: PathBuf,
    machine: String,
    elf: PathBuf,
    /// The link UART, when the plan's link is serial.
    link: Option<String>,
    console: Option<String>,
}

fn twin(plan: &RunPlan) -> Result<Twin, String> {
    let machine = match plan.target.emulator.as_deref() {
        Some(e) => match e.strip_prefix("renode:") {
            Some(m) if !m.is_empty() => m.to_string(),
            _ => {
                return Err(format!(
                    "the plan's target is `{e}`; the renode runner boots `renode:<machine>`"
                ));
            }
        },
        None => return Err("the plan names no emulator; the renode runner boots one".into()),
    };
    let elf = plan
        .artifacts
        .get("elf")
        .cloned()
        .ok_or("the plan carries no `elf` artifact to boot")?;
    let renode = plan.tools.get("renode").cloned().ok_or(
        "the plan's tools name no `renode`; nros resolves it through `nros locate tool renode` \
         (`$NROS_RENODE`, then the store: `nros setup --tool renode`), and this runner never \
         takes one from PATH",
    )?;
    let serial = plan
        .link
        .as_ref()
        .is_some_and(|l| l.kind == LinkKind::Serial);
    let link = match (serial, plan.args.get("link")) {
        (true, Some(l)) => Some(l.clone()),
        (true, None) => {
            return Err(
                "the image's link is serial and the plan names no `link` UART; the board's \
                 `[board.run.renode] link` says which `sysbus` peripheral carries it"
                    .into(),
            );
        }
        (false, _) => None,
    };
    Ok(Twin {
        renode,
        machine,
        elf,
        link,
        console: plan.args.get("console").cloned(),
    })
}

/// The Renode script: the machine, the link on a pty, the console into a
/// file, the ELF. Paths are quoted with `@` as Renode's monitor expects.
fn script(t: &Twin, run: &Path) -> String {
    let mut s = vec![
        "mach create \"twin\"".to_string(),
        format!(
            "machine LoadPlatformDescription @platforms/boards/{}.repl",
            t.machine
        ),
    ];
    if let Some(link) = &t.link {
        s.push(format!(
            "emulation CreateUartPtyTerminal \"zlink\" \"{}\" true",
            run.join("link").display()
        ));
        s.push(format!("connector Connect sysbus.{link} zlink"));
    }
    if let Some(c) = &t.console {
        s.push(format!(
            "sysbus.{c} CreateFileBackend @{} true",
            run.join("console.log").display()
        ));
    }
    s.push(format!("sysbus LoadELF @{}", t.elf.display()));
    s.join("\n") + "\n"
}

/// A free TCP port on loopback for Renode's monitor.
fn free_port() -> std::io::Result<u16> {
    Ok(TcpListener::bind("127.0.0.1:0")?.local_addr()?.port())
}

/// Send one monitor command, once the monitor answers. The connection is
/// returned for the caller to hold while Renode runs: closing it makes Renode
/// log a `Connection reset by peer` stack trace (measured), which reads like a
/// failure and is none.
fn monitor(port: u16, cmd: &str, within: Duration) -> Result<TcpStream, String> {
    let deadline = Instant::now() + within;
    loop {
        match TcpStream::connect(("127.0.0.1", port)) {
            Ok(mut s) => {
                writeln!(s, "{cmd}").map_err(|e| format!("monitor write: {e}"))?;
                return Ok(s);
            }
            Err(e) if Instant::now() >= deadline => {
                return Err(format!("Renode's monitor on 127.0.0.1:{port}: {e}"));
            }
            Err(_) => std::thread::sleep(Duration::from_millis(250)),
        }
    }
}

/// Copy `path` to stdout as it grows, until `done` is set and the file has
/// been read to its end.
fn follow(path: &Path, done: &AtomicBool) {
    let mut out = std::io::stdout();
    let mut f = None;
    let mut buf = [0u8; 4096];
    loop {
        let finished = done.load(Ordering::SeqCst);
        if f.is_none() {
            f = File::open(path).ok();
        }
        if let Some(file) = f.as_mut() {
            loop {
                match file.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let _ = out.write_all(&buf[..n]).and_then(|()| out.flush());
                    }
                }
            }
        }
        if finished {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn stop(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn run(plan: &RunPlan) -> RunnerExit {
    let t = match twin(plan) {
        Ok(t) => t,
        Err(e) => return runner::fail(e),
    };
    let run = std::env::temp_dir().join(format!("nros-runner-renode-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&run);
    if let Err(e) = std::fs::create_dir_all(run.join("home/.config")) {
        return runner::fail(format!("creating {}: {e}", run.display()));
    }
    let resc = run.join("twin.resc");
    if let Err(e) = std::fs::write(&resc, script(&t, &run)) {
        return runner::fail(format!("writing {}: {e}", resc.display()));
    }
    let port = match free_port() {
        Ok(p) => p,
        Err(e) => return runner::fail(format!("no free port for Renode's monitor: {e}")),
    };
    let renode = std::fs::canonicalize(&t.renode).unwrap_or_else(|_| t.renode.clone());
    let dir = renode.parent().map(Path::to_path_buf).unwrap_or_default();
    let (out, err) = match stderr_twice() {
        Ok(p) => p,
        Err(e) => return runner::fail(format!("duplicating stderr: {e}")),
    };
    let mut child = match Command::new(&renode)
        .args(["--disable-gui", "--plain", "-P", &port.to_string()])
        .arg(&resc)
        .current_dir(&dir)
        .env("HOME", run.join("home"))
        .env("XDG_CONFIG_HOME", run.join("home/.config"))
        .stdin(Stdio::null())
        // Renode's own log is a diagnostic: the console is the UART file.
        .stdout(out)
        .stderr(err)
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return runner::fail(format!("could not start {}: {e}", renode.display())),
    };

    let done = Arc::new(AtomicBool::new(false));
    let follower = t.console.as_ref().map(|_| {
        let path = run.join("console.log");
        let done = Arc::clone(&done);
        std::thread::spawn(move || follow(&path, &done))
    });

    if t.link.is_some() {
        let pty = run.join("link");
        let deadline = Instant::now() + PTY_WITHIN;
        while !pty.exists() {
            if matches!(child.try_wait(), Ok(Some(_))) || Instant::now() >= deadline {
                stop(&mut child);
                return runner::fail(format!(
                    "Renode did not create the link pty {} (its log is above)",
                    pty.display()
                ));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        console::report_link(&pty.display().to_string());
        std::thread::sleep(SERIAL_SETTLE);
    }
    let _monitor = match monitor(port, "start", PTY_WITHIN) {
        Ok(m) => m,
        Err(e) => {
            stop(&mut child);
            return runner::fail(format!("starting the simulation: {e}"));
        }
    };

    let status = child.wait();
    done.store(true, Ordering::SeqCst);
    if let Some(f) = follower {
        let _ = f.join();
    }
    let _ = std::fs::remove_dir_all(&run);
    match status {
        Ok(s) if s.success() => RunnerExit::Success,
        Ok(_) => RunnerExit::TargetFailed,
        Err(e) => runner::fail(format!("waiting for {}: {e}", renode.display())),
    }
}

/// This process's stderr, duplicated twice, as a child's stdout and stderr.
fn stderr_twice() -> std::io::Result<(Stdio, Stdio)> {
    use std::os::fd::AsFd;
    let a = std::io::stderr().as_fd().try_clone_to_owned()?;
    let b = a.try_clone()?;
    Ok((Stdio::from(a), Stdio::from(b)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(link: bool, args: &str, tools: &str) -> RunPlan {
        let link = if link {
            r#","link":{"kind":"serial","locator":"serial/serial@40004800#baudrate=115200"}"#
        } else {
            ""
        };
        RunPlan::from_json(&format!(
            r#"{{"image":"i","verb":"run","runner":{{"package":"nros_runner_renode","name":"renode"}},
            "target":{{"emulator":"renode:nucleo_f767zi"}},"console":{{"kind":"serial"}},
            "artifacts":{{"elf":"/b/zephyr/zephyr.elf"}},"args":{args},"tools":{tools}{link}}}"#
        ))
        .unwrap()
    }

    #[test]
    fn describe_agrees_with_the_descriptor() {
        runner::descriptor_agrees(&describe(), include_str!("../nros-runner.toml")).unwrap();
    }

    #[test]
    fn the_script_is_the_boards_wiring() {
        let p = plan(
            true,
            r#"{"link":"usart3","console":"usart6"}"#,
            r#"{"renode":"/store/renode/renode"}"#,
        );
        let t = twin(&p).unwrap();
        assert_eq!(t.machine, "nucleo_f767zi");
        assert_eq!(t.renode, PathBuf::from("/store/renode/renode"));
        let s = script(&t, Path::new("/r"));
        assert_eq!(
            s,
            "mach create \"twin\"\n\
             machine LoadPlatformDescription @platforms/boards/nucleo_f767zi.repl\n\
             emulation CreateUartPtyTerminal \"zlink\" \"/r/link\" true\n\
             connector Connect sysbus.usart3 zlink\n\
             sysbus.usart6 CreateFileBackend @/r/console.log true\n\
             sysbus LoadELF @/b/zephyr/zephyr.elf\n"
        );
    }

    #[test]
    fn no_serial_link_no_pty() {
        let p = plan(false, r#"{"link":"usart3"}"#, r#"{"renode":"/r"}"#);
        let t = twin(&p).unwrap();
        assert_eq!(t.link, None);
        assert!(!script(&t, Path::new("/r")).contains("CreateUartPtyTerminal"));
    }

    #[test]
    fn a_serial_link_with_no_uart_is_refused() {
        let e = twin(&plan(true, "{}", r#"{"renode":"/r"}"#)).unwrap_err();
        assert!(e.contains("[board.run.renode] link"), "{e}");
    }

    #[test]
    fn no_tool_no_path_fallback() {
        let e = twin(&plan(true, r#"{"link":"usart3"}"#, "{}")).unwrap_err();
        assert!(e.contains("nros locate tool renode"), "{e}");
    }

    #[test]
    fn another_emulator_is_refused() {
        let mut p = plan(true, r#"{"link":"usart3"}"#, r#"{"renode":"/r"}"#);
        p.target.emulator = Some("qemu:mps2-an385".into());
        assert!(twin(&p).unwrap_err().contains("renode:<machine>"));
    }
}
