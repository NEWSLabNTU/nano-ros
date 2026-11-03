//! `nros flash`: program a real board, then optionally watch its console
//! (RFC-0118 D2, D6, D7).
//!
//! 1. Resolve the flash plan for the one image the selection names
//!    ([`run_plan::resolve_target`], the target resolution every target verb
//!    shares), with the port and probe from RFC-0118 D6's ladder
//!    ([`run_plan::bench_for`]); or read `--plan <file>` as written.
//! 2. Build the image if it is stale (not under `--plan`).
//! 3. Run the plan's `prepare` steps ([`crate::cmd::prepare`]).
//! 4. Flash through the plan's runner.
//! 5. With `--monitor`, hand the same port to the console runner
//!    ([`run_plan::console_of`]) and grade `--expect` over the console.
//!
//! Steps 4 and 5 go through [`crate::cmd::run_exec::execute`], so a flash
//! and a console get `nros run`'s process group, capture, grading and exit
//! codes; nothing here reimplements them.

use std::path::PathBuf;

use nros_extension::{
    exit,
    plan::{RunPlan, Verb},
};

use crate::cmd::{
    run_exec::{self, ExecArgs, Execution},
    run_plan::{self, Inputs, Invocation, Selection},
};

/// `nros flash` — program an image onto a real board.
#[derive(clap::Args, Debug, Clone)]
pub struct Args {
    /// The image to flash (`<image>` or `<bringup>:<image>`), as `nros build`
    /// names it. Omitted: the workspace's one image.
    pub image: Option<String>,
    /// Workspace root (auto-detected from cwd if omitted).
    #[arg(long)]
    pub workspace: Option<PathBuf>,
    /// nano-ros checkout (auto-detected if omitted).
    #[arg(long)]
    pub nano_ros_path: Option<PathBuf>,
    /// Print the resolved plan(s) and do nothing.
    #[arg(long)]
    pub dry_run: bool,
    /// Add the ladder rung that supplied each field.
    #[arg(long)]
    pub why: bool,
    /// Print the plan as the JSON a runner is handed, not TOML.
    #[arg(long)]
    pub json: bool,
    /// The board's serial device (else `NROS_PORT`, the bench file, or the
    /// one attached device matching the board).
    #[arg(long)]
    pub port: Option<String>,
    /// The debug probe's serial number.
    #[arg(long)]
    pub probe: Option<String>,
    /// The flash method's own runner (`west flash --runner jlink`).
    #[arg(long)]
    pub runner: Option<String>,
    /// Never wait for a person: a step that needs one fails instead.
    #[arg(long)]
    pub no_prompt: bool,
    /// After flashing, watch the board's console on the same port.
    #[arg(long)]
    pub monitor: bool,
    /// With `--monitor`: stop with exit 0 once a console line contains TEXT.
    #[arg(long, value_name = "TEXT", conflicts_with = "expect_regex")]
    pub expect: Option<String>,
    /// As `--expect`, with a regular expression over one console line.
    #[arg(long, value_name = "REGEX")]
    pub expect_regex: Option<String>,
    /// Stop after SECONDS: the console with `--monitor`, and any wait a
    /// prepare step makes.
    #[arg(long, value_name = "SECONDS")]
    pub timeout: Option<f64>,
    /// With `--monitor`: what serves the image's link on this host (`auto`,
    /// `none`, or a running router's locator).
    #[arg(long, value_name = "auto|none|LOCATOR", default_value = "auto")]
    pub router: String,
    /// Flash this plan file as written: resolve nothing, build nothing.
    #[arg(long, value_name = "FILE")]
    pub plan: Option<PathBuf>,
    /// Arguments for the flasher, after `--`.
    #[arg(last = true)]
    pub passthrough: Vec<String>,
}

impl Args {
    fn selection(&self) -> Selection {
        Selection {
            workspace: self.workspace.clone(),
            nano_ros_path: self.nano_ros_path.clone(),
            image: self.image.clone(),
        }
    }

    /// The flash phase: no grading, no router.
    fn flash_exec(&self) -> ExecArgs {
        ExecArgs {
            router: "none".into(),
            ..ExecArgs::default()
        }
    }

    /// The console phase: the user's grading and router.
    fn console_exec(&self) -> ExecArgs {
        ExecArgs {
            expect: self.expect.clone(),
            expect_regex: self.expect_regex.clone(),
            timeout: self.timeout,
            router: self.router.clone(),
            ..ExecArgs::default()
        }
    }
}

fn exit_with(code: u8) -> ! {
    use std::io::Write;
    let _ = std::io::stdout().flush();
    std::process::exit(i32::from(code));
}

fn unresolved(msg: &str) -> ! {
    eprintln!("nros flash: {msg}");
    exit_with(exit::PLAN_UNRESOLVED);
}

/// The flash plan, and the console plan `--monitor` opens after it.
fn plans(args: &Args) -> (RunPlan, Option<crate::cmd::build::ResolvedBuild>) {
    if let Some(file) = &args.plan {
        let mut plan = RunPlan::read(file).unwrap_or_else(|e| unresolved(&e.to_string()));
        if plan.verb != Verb::Flash {
            unresolved(&format!(
                "{} is a `{}` plan; `nros flash` executes `flash` plans",
                file.display(),
                plan.verb
            ));
        }
        if let Some(p) = &args.port {
            plan.console.port = Some(p.clone());
        }
        if !args.passthrough.is_empty() {
            plan.passthrough = args.passthrough.clone();
        }
        return (plan, None);
    }
    let one =
        run_plan::resolve_target(&args.selection(), "flash").unwrap_or_else(|e| unresolved(&e));
    let descriptor = one.descriptor.as_ref().expect("resolve_target checked it");
    let invocation = Invocation {
        // RFC-0118 D6: `--port`, then `NROS_PORT`, both the invocation rung.
        port: args
            .port
            .clone()
            .or_else(|| std::env::var("NROS_PORT").ok().filter(|p| !p.is_empty())),
        probe: args.probe.clone(),
        runner: args.runner.clone(),
        passthrough: args.passthrough.clone(),
    };
    let machine = crate::cmd::bench::Machine::load()
        .unwrap_or_else(|e| unresolved(&format!("reading this machine's bench: {e:#}")));
    let bench = run_plan::bench_for(
        Verb::Flash,
        &one.board,
        descriptor,
        &one.run_tables,
        &invocation,
        &machine,
    )
    .unwrap_or_else(|e| unresolved(&e.to_string()));
    let plan = run_plan::resolve(
        Verb::Flash,
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
    .unwrap_or_else(|e| unresolved(&e.to_string()));
    (plan, Some(one))
}

/// The console plan for `--monitor`: the flash plan's board on the same port.
#[must_use]
pub fn monitor_plan(flash: &RunPlan) -> RunPlan {
    let mut c = flash.clone();
    run_plan::console_of(&mut c);
    // A flasher's arguments are not the console's.
    c.passthrough.clear();
    c
}

/// `nros flash`.
///
/// # Errors
///
/// Never returns an error; every failure is an exit code (RFC-0118 D2).
pub fn run(args: Args) -> eyre::Result<()> {
    let (plan, one) = plans(&args);
    let console = args.monitor.then(|| monitor_plan(&plan));
    if args.dry_run {
        print!("{}", run_plan::render_plan_text(&plan, args.why, args.json));
        if let Some(c) = &console {
            if !args.json {
                println!("\n# --monitor: then the console");
            }
            print!("{}", run_plan::render_plan_text(c, args.why, args.json));
        }
        return Ok(());
    }
    let sel = args.selection();
    let ws = sel
        .workspace
        .clone()
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));
    let ws = ws.canonicalize().unwrap_or(ws);
    let nano_ros_root =
        crate::orchestration::nano_ros_root::resolve(sel.nano_ros_path.clone(), &ws);
    if let Some(one) = &one
        && let Err(e) = run_exec::build_image(&sel, &one.qualified)
    {
        eprintln!("nros flash: {e}");
        exit_with(exit::RUNNER_FAILED);
    }
    exit_with(flash_and_monitor(
        &plan,
        console.as_ref(),
        &args,
        &Execution {
            ws: &ws,
            nano_ros_root: nano_ros_root.as_deref(),
            exec: &args.flash_exec(),
            runner: None,
        },
        &crate::cmd::prepare::Host::real(),
    ));
}

/// Prepare, flash, then (with a console plan) watch. Returns the exit code.
/// `x.runner`, when set, is used for both phases (a test's fake runner).
pub fn flash_and_monitor(
    plan: &RunPlan,
    console: Option<&RunPlan>,
    args: &Args,
    x: &Execution<'_>,
    host: &crate::cmd::prepare::Host,
) -> u8 {
    let timeout = match args.timeout {
        Some(t) if t.is_finite() && t > 0.0 => Some(std::time::Duration::from_secs_f64(t)),
        Some(t) => {
            eprintln!("nros flash: `--timeout {t}` must be a positive number of seconds");
            return exit::PLAN_UNRESOLVED;
        }
        None => None,
    };
    let opts = crate::cmd::prepare::Options::from_verb(args.no_prompt, timeout);
    if let Err(e) =
        crate::cmd::prepare::execute(&plan.prepare, &opts, host, &mut std::io::stderr(), None)
    {
        eprintln!("nros flash: {e}");
        return e.exit_code();
    }
    let code = run_exec::execute(plan, x);
    if code != exit::SUCCESS {
        return code;
    }
    let Some(c) = console else {
        return code;
    };
    let console_exec = args.console_exec();
    run_exec::execute(
        c,
        &Execution {
            exec: &console_exec,
            ..x.clone()
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use nros_extension::plan::{Console, ConsoleKind, PrepareStep, RunnerRef, Target};
    use std::{collections::BTreeMap, path::Path};

    fn flash_plan(port: &str) -> RunPlan {
        RunPlan {
            protocol: 1,
            image: "blinky".into(),
            artifacts: BTreeMap::new(),
            build_dir: None,
            verb: Verb::Flash,
            runner: RunnerRef {
                package: "nros_runner_fake".into(),
                name: "fake".into(),
            },
            target: Target {
                flasher: Some("fake".into()),
                ..Target::default()
            },
            console: Console {
                kind: ConsoleKind::Serial,
                port: Some(port.into()),
                baud: Some(115_200),
                command: None,
            },
            link: None,
            args: BTreeMap::new(),
            tools: BTreeMap::new(),
            passthrough: vec!["--erase".into()],
            prepare: Vec::new(),
            provenance: BTreeMap::new(),
        }
    }

    fn args() -> Args {
        Args {
            image: None,
            workspace: None,
            nano_ros_path: None,
            dry_run: false,
            why: false,
            json: false,
            port: None,
            probe: None,
            runner: None,
            no_prompt: true,
            monitor: true,
            expect: None,
            expect_regex: None,
            timeout: Some(5.0),
            router: "none".into(),
            plan: None,
            passthrough: Vec::new(),
        }
    }

    /// A fake runner for both verbs. `flash` records its plan's port and
    /// passthrough in `<dir>/flashed`; `console` prints the port it was
    /// handed, then the board's banner.
    fn fake(dir: &Path, flash_exit: u8) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let p = dir.join("fake-runner");
        std::fs::write(
            &p,
            format!(
                "#!/bin/sh\n\
                 if [ \"$1\" = describe ]; then\n\
                 echo '{{\"protocol\":[1],\"verbs\":[\"flash\",\"console\"],\"needs\":[]}}'; exit 0\n\
                 fi\n\
                 port=$(sed -n 's/.*\"port\": \"\\([^\"]*\\)\".*/\\1/p' \"$3\")\n\
                 case \"$1\" in\n\
                 flash) grep -c -- '--erase' \"$3\" > {d}/erased; echo \"$port\" > {d}/flashed; \
                 echo flashing; exit {flash_exit} ;;\n\
                 console) grep -c -- '--erase' \"$3\" > {d}/console-erased; \
                 echo \"console on $port\"; echo 'Booted: ready'; sleep 30 ;;\n\
                 esac\n\
                 exit 2\n",
                d = dir.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p
    }

    fn go(dir: &Path, plan: &RunPlan, a: &Args, runner: PathBuf) -> u8 {
        let _g = crate::target_session::tests::FLAG
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let console = a.monitor.then(|| monitor_plan(plan));
        let flash = a.flash_exec();
        flash_and_monitor(
            plan,
            console.as_ref(),
            a,
            &Execution {
                ws: dir,
                nano_ros_root: None,
                exec: &flash,
                runner: Some(runner),
            },
            &crate::cmd::prepare::Host::under(dir),
        )
    }

    #[test]
    fn flash_then_monitor_hands_on_the_same_port() {
        let d = tempfile::tempdir().unwrap();
        let r = fake(d.path(), 0);
        let mut a = args();
        a.expect = Some("Booted: ready".into());
        assert_eq!(go(d.path(), &flash_plan("/dev/ttyACM7"), &a, r), 0);
        let read = |f: &str| std::fs::read_to_string(d.path().join(f)).unwrap();
        assert_eq!(read("flashed").trim(), "/dev/ttyACM7");
        // The flasher got the arguments after `--`; the console did not.
        assert_eq!(read("erased").trim(), "1");
        assert_eq!(read("console-erased").trim(), "0");
        let run = run_exec::run_dir(d.path(), "blinky");
        let console = std::fs::read_to_string(run.join("console.log")).unwrap();
        assert!(console.contains("console on /dev/ttyACM7"), "{console}");
        assert!(!console.contains("flashing"), "{console}");
        let flash_log = std::fs::read_to_string(run.join("flash.log")).unwrap();
        assert!(flash_log.contains("flashing"), "{flash_log}");
    }

    #[test]
    fn a_failed_flash_is_exit_two_and_opens_no_console() {
        let d = tempfile::tempdir().unwrap();
        let r = fake(d.path(), 2);
        assert_eq!(go(d.path(), &flash_plan("/dev/ttyACM7"), &args(), r), 2);
        assert!(!d.path().join("console-erased").exists());
    }

    #[test]
    fn a_prepare_step_needing_a_person_fails_under_no_prompt() {
        let d = tempfile::tempdir().unwrap();
        let r = fake(d.path(), 0);
        let mut p = flash_plan("/dev/ttyACM7");
        p.prepare.push(PrepareStep::Say(
            "Hold BOOTSEL, then plug the board in".into(),
        ));
        assert_eq!(go(d.path(), &p, &args(), r), 2);
        assert!(
            !d.path().join("flashed").exists(),
            "flashed after a failed prepare"
        );
    }

    #[test]
    fn the_console_plan_is_the_serial_runner_on_the_flash_port() {
        let mut f = flash_plan("/dev/ttyUSB3");
        f.args.insert("command".into(), "pyocd flash {elf}".into());
        let c = monitor_plan(&f);
        assert_eq!(c.verb, Verb::Console);
        assert_eq!(c.runner.name, "serial");
        assert_eq!(c.console.port.as_deref(), Some("/dev/ttyUSB3"));
        assert!(c.passthrough.is_empty());
        assert!(!c.args.contains_key("command"));
        assert!(c.validate().is_ok());
    }
}
