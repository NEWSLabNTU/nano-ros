//! `nros monitor`: open a running board's console (RFC-0118 D2, D6).
//!
//! In a workspace the console plan is the image's board's
//! ([`run_plan::select_target`], the target resolution every target verb
//! shares, then the `console` verb of [`run_plan::resolve`]), with the port
//! from RFC-0118 D6's ladder. Outside one, `--port` alone is enough: a board
//! that is already running needs no image to be watched. Either way the plan
//! goes through [`crate::cmd::run_exec::execute`], so `--expect`, the
//! capture to `console.log` and the teardown are `nros run`'s.

use std::{collections::BTreeMap, path::PathBuf};

use nros_extension::{
    exit,
    plan::{Console, ConsoleKind, RunPlan, RunnerRef, Target, Verb},
};

use crate::cmd::{
    run_exec::{self, ExecArgs, Execution},
    run_plan::{self, INVOCATION, Inputs, Invocation, Selection},
};

/// `nros monitor` — attach to a running board's console.
#[derive(clap::Args, Debug, Clone)]
pub struct Args {
    /// The image whose board to watch (`<image>` or `<bringup>:<image>`).
    /// Omitted: the workspace's one image, or none with `--port`.
    pub image: Option<String>,
    /// Workspace root (auto-detected from cwd if omitted).
    #[arg(long)]
    pub workspace: Option<PathBuf>,
    /// nano-ros checkout (auto-detected if omitted).
    #[arg(long)]
    pub nano_ros_path: Option<PathBuf>,
    /// The board's serial device (else `NROS_PORT`, the bench file, or the
    /// one attached device matching the board).
    #[arg(long)]
    pub port: Option<String>,
    /// The serial baud rate (else the board's).
    #[arg(long)]
    pub baud: Option<u32>,
    /// Stop with exit 0 once a console line contains TEXT; exit 1 if
    /// `--timeout` passes first.
    #[arg(long, value_name = "TEXT", conflicts_with = "expect_regex")]
    pub expect: Option<String>,
    /// As `--expect`, with a regular expression over one console line.
    #[arg(long, value_name = "REGEX")]
    pub expect_regex: Option<String>,
    /// Stop after SECONDS.
    #[arg(long, value_name = "SECONDS")]
    pub timeout: Option<f64>,
    /// Send LINE to the board once the console is open (repeatable).
    #[arg(long, value_name = "LINE")]
    pub send: Vec<String>,
    /// Print the resolved plan and open nothing.
    #[arg(long)]
    pub dry_run: bool,
    /// Add the ladder rung that supplied each field.
    #[arg(long)]
    pub why: bool,
    /// Print the plan as the JSON a runner is handed, not TOML.
    #[arg(long)]
    pub json: bool,
    /// `--base-paths`, as `nros build` takes it (phase-527 W2d).
    #[command(flatten)]
    pub sources: crate::cmd::build::BasePathsArgs,
}

fn exit_with(code: u8) -> ! {
    use std::io::Write;
    let _ = std::io::stdout().flush();
    std::process::exit(i32::from(code));
}

fn unresolved(msg: &str) -> ! {
    eprintln!("nros monitor: {msg}");
    exit_with(exit::PLAN_UNRESOLVED);
}

/// A console plan for a port and nothing else: no workspace names the board.
#[must_use]
pub fn bare_plan(port: &str) -> RunPlan {
    let mut provenance = BTreeMap::new();
    provenance.insert("console.port".to_string(), INVOCATION.to_string());
    RunPlan {
        protocol: nros_extension::protocol::PROTOCOL,
        image: "monitor".into(),
        artifacts: BTreeMap::new(),
        build_dir: None,
        verb: Verb::Console,
        runner: RunnerRef {
            package: "nros_runner_serial".into(),
            name: "serial".into(),
        },
        // The console of a board this command did not flash: its flasher is
        // unknown and nothing reads it.
        target: Target {
            flasher: Some("none".into()),
            ..Target::default()
        },
        console: Console {
            kind: ConsoleKind::Serial,
            port: Some(port.to_string()),
            baud: None,
            command: None,
        },
        link: None,
        args: BTreeMap::new(),
        tools: BTreeMap::new(),
        passthrough: Vec::new(),
        prepare: Vec::new(),
        provenance,
    }
}

/// Apply `--baud` and `--send` (the invocation rung) to a console plan.
pub fn apply_invocation(plan: &mut RunPlan, baud: Option<u32>, send: &[String]) {
    if let Some(b) = baud {
        plan.console.baud = Some(b);
        plan.provenance
            .insert("console.baud".into(), INVOCATION.into());
    }
    if !send.is_empty() {
        plan.args.insert("send".into(), send.join("\n"));
        plan.provenance
            .insert("args.send".into(), INVOCATION.into());
    }
}

fn resolve_plan(args: &Args) -> RunPlan {
    let port = args
        .port
        .clone()
        .or_else(|| std::env::var("NROS_PORT").ok().filter(|p| !p.is_empty()));
    let sel = Selection {
        workspace: args.workspace.clone(),
        nano_ros_path: args.nano_ros_path.clone(),
        image: args.image.clone(),
        base_paths: args.sources.base_paths.clone(),
    };
    let one = match run_plan::select_target(&sel, "monitor") {
        Ok(one) => one,
        // No image to name the board, but a port to watch: enough.
        Err(_) if args.image.is_none() && port.is_some() => {
            return bare_plan(port.as_deref().expect("checked"));
        }
        Err(e) => unresolved(&format!(
            "{e}. Outside a workspace, name the board's device with `--port`"
        )),
    };
    let descriptor = one.descriptor.as_ref().expect("select_target checked it");
    let invocation = Invocation {
        port,
        ..Invocation::default()
    };
    let machine = crate::cmd::bench::Machine::load()
        .unwrap_or_else(|e| unresolved(&format!("reading this machine's bench: {e:#}")));
    let bench = run_plan::bench_for(
        Verb::Console,
        &one.board,
        descriptor,
        &one.run_tables,
        &invocation,
        &machine,
    )
    .unwrap_or_else(|e| unresolved(&e.to_string()));
    run_plan::resolve(
        Verb::Console,
        Inputs {
            image: &one.qualified,
            board: &one.board,
            descriptor,
            output: one.output.as_ref(),
            tables: &one.run_tables,
            bench: &bench,
            invocation: &invocation,
        },
    )
    .unwrap_or_else(|e| unresolved(&e.to_string()))
}

/// `nros monitor`.
///
/// # Errors
///
/// Never returns an error; every failure is an exit code (RFC-0118 D2).
pub fn run(args: Args) -> eyre::Result<()> {
    let mut plan = resolve_plan(&args);
    apply_invocation(&mut plan, args.baud, &args.send);
    if args.dry_run {
        print!("{}", run_plan::render_plan_text(&plan, args.why, args.json));
        return Ok(());
    }
    let ws = args
        .workspace
        .clone()
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));
    let ws = ws.canonicalize().unwrap_or(ws);
    let nano_ros_root =
        crate::orchestration::nano_ros_root::resolve(args.nano_ros_path.clone(), &ws);
    let exec = ExecArgs {
        expect: args.expect.clone(),
        expect_regex: args.expect_regex.clone(),
        timeout: args.timeout,
        // A console attaches to a board already running; it starts nothing.
        router: "none".into(),
        sources: args.sources.clone(),
        ..ExecArgs::default()
    };
    exit_with(run_exec::execute(
        &plan,
        &Execution {
            ws: &ws,
            nano_ros_root: nano_ros_root.as_deref(),
            exec: &exec,
            runner: None,
        },
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs::File,
        io::{Read, Write},
        os::fd::{FromRawFd, OwnedFd},
        path::Path,
    };

    /// A pty pair: the controller side plays the board, the device path is
    /// the port the console opens.
    fn pty() -> (File, String) {
        // SAFETY: plain libc calls on a descriptor this function owns; `name`
        // is a buffer `ptsname_r` fills with a NUL-terminated path.
        unsafe {
            let fd = libc::posix_openpt(libc::O_RDWR | libc::O_NOCTTY);
            assert!(fd >= 0, "posix_openpt: {}", std::io::Error::last_os_error());
            assert_eq!(libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC), 0);
            assert_eq!(libc::grantpt(fd), 0);
            assert_eq!(libc::unlockpt(fd), 0);
            let mut name = [0 as libc::c_char; 128];
            assert_eq!(libc::ptsname_r(fd, name.as_mut_ptr(), name.len()), 0);
            let path = std::ffi::CStr::from_ptr(name.as_ptr())
                .to_string_lossy()
                .into_owned();
            (File::from(OwnedFd::from_raw_fd(fd)), path)
        }
    }

    /// A console runner in shell: open the plan's port, send `args.send`, and
    /// copy what the board says to stdout.
    fn fake_console(dir: &Path) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let p = dir.join("fake-serial");
        std::fs::write(
            &p,
            "#!/bin/sh\n\
             if [ \"$1\" = describe ]; then\n\
             echo '{\"protocol\":[1],\"verbs\":[\"console\"],\"needs\":[\"port\"]}'; exit 0\n\
             fi\n\
             [ \"$1\" = console ] || exit 2\n\
             port=$(sed -n 's/.*\"port\": \"\\([^\"]*\\)\".*/\\1/p' \"$3\")\n\
             send=$(sed -n 's/.*\"send\": \"\\([^\"]*\\)\".*/\\1/p' \"$3\")\n\
             stty -F \"$port\" raw -echo\n\
             exec 3<>\"$port\"\n\
             printf '%s\\n' \"$send\" >&3\n\
             exec cat <&3\n",
        )
        .unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p
    }

    #[test]
    fn monitor_sends_and_grades_against_a_pty() {
        let _g = crate::target_session::tests::FLAG
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let d = tempfile::tempdir().unwrap();
        let (mut board, port) = pty();
        // Hold the device side open for the test's whole life: reading the
        // controller side before anything has opened the device is EIO.
        let _held = {
            use std::os::unix::fs::OpenOptionsExt;
            File::options()
                .read(true)
                .write(true)
                .custom_flags(libc::O_NOCTTY)
                .open(&port)
                .unwrap()
        };
        let runner = fake_console(d.path());
        // The board answers `status` with its banner, as a shell would.
        let reader = std::thread::spawn(move || {
            let mut got = Vec::new();
            let mut buf = [0u8; 64];
            while !got.ends_with(b"status\n") {
                let n = board.read(&mut buf).unwrap();
                got.extend_from_slice(&buf[..n]);
            }
            board.write_all(b"boot ok\r\nstate: ready\r\n").unwrap();
            board.flush().unwrap();
            std::thread::sleep(std::time::Duration::from_secs(10));
            got
        });
        let mut plan = bare_plan(&port);
        apply_invocation(&mut plan, Some(115_200), &["status".to_string()]);
        let exec = ExecArgs {
            expect: Some("state: ready".into()),
            timeout: Some(10.0),
            router: "none".into(),
            ..ExecArgs::default()
        };
        let code = run_exec::execute(
            &plan,
            &Execution {
                ws: d.path(),
                nano_ros_root: None,
                exec: &exec,
                runner: Some(runner),
            },
        );
        assert_eq!(code, 0);
        let log =
            std::fs::read_to_string(run_exec::run_dir(d.path(), "monitor").join("console.log"))
                .unwrap();
        assert!(log.contains("state: ready"), "{log}");
        drop(reader);
    }

    #[test]
    fn the_invocation_sets_baud_and_send() {
        let mut p = bare_plan("/dev/ttyACM0");
        apply_invocation(&mut p, Some(921_600), &["a".into(), "b".into()]);
        assert_eq!(p.console.baud, Some(921_600));
        assert_eq!(p.args["send"], "a\nb");
        assert_eq!(p.provenance["args.send"], INVOCATION);
        assert!(p.validate().is_ok());
    }
}
