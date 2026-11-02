//! phase-529 W1 — executing a RunPlan's `prepare` steps (RFC-0118 D7).
//!
//! A step is `say` (print an instruction) or `wait_for` (block until an
//! event). The plan contract's `PrepareStep` has no `run` step yet, so the
//! resolver still refuses one ([`crate::cmd::run_plan`]); when
//! `nros_extension::plan::PrepareStep::Run` lands, its arm goes here.
//!
//! # Who runs these
//!
//! nros does, before it hands the plan to a runner. RFC-0119 D6 lists
//! "`prepare` steps and `--no-prompt`" among what nros owns so that every
//! runner behaves the same, so the executor lives in `nros-cli-core`, not in
//! `nros-extension`: a runner is asked to start or flash, never to talk to a
//! person.
//!
//! # The rules
//!
//! * With no terminal on stdin, or with `--no-prompt`, a `say` step fails the
//!   verb with exit 2 ([`nros_extension::exit::RUNNER_FAILED`]) and prints the
//!   instruction on stderr. A lab script or CI job gets a clear failure
//!   instead of a hang.
//! * A `wait_for` polls until its event holds, for at most the verb's
//!   `--timeout`, else [`DEFAULT_TIMEOUT`]. Timing out is exit 2 too: the
//!   runner could not do its work because the step did not happen.
//! * Events, all read under a [`Host`] root so tests need no hardware:
//!   - `usb:<vid>:<pid>` — a device under `<sys>/bus/usb/devices/*` whose
//!     `idVendor` / `idProduct` match (a DFU or mass-storage device is not a
//!     tty, so the serial-port lookup in `nros_extension::usb` cannot see it);
//!   - `port:<path>` — the path exists;
//!   - `volume:<label>` — `<dev>/disk/by-label/<label>` exists and the device
//!     it names is mounted (a source in `<proc>/mounts`);
//!   - `line:<text>` — a console line holding `<text>`. Prepare runs BEFORE
//!     the target starts, so there is no console then: the step is refused
//!     unless the caller hands one in ([`Console`]), which only a verb that
//!     opens the console first can do (`nros flash --monitor`, W3).

use std::{
    io::{BufRead, Write},
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

use nros_extension::plan::PrepareStep;
use nros_orchestration_ir::target_exec::WaitFor;

/// How long a `wait_for` waits when the verb gives no `--timeout`: long
/// enough for a person to set a jumper and press a button.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(300);

/// Where the events are looked for — the real host, or a fake tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Host {
    /// sysfs (`/sys`).
    pub sys: PathBuf,
    /// devtmpfs (`/dev`).
    pub dev: PathBuf,
    /// procfs (`/proc`).
    pub proc: PathBuf,
}

impl Host {
    /// This machine.
    #[must_use]
    pub fn real() -> Self {
        Self {
            sys: "/sys".into(),
            dev: "/dev".into(),
            proc: "/proc".into(),
        }
    }

    /// A fake tree under `root` (`root/sys`, `root/dev`, `root/proc`).
    #[must_use]
    pub fn under(root: &Path) -> Self {
        Self {
            sys: root.join("sys"),
            dev: root.join("dev"),
            proc: root.join("proc"),
        }
    }
}

/// How the verb was invoked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// `--no-prompt`.
    pub no_prompt: bool,
    /// Whether stdin is a terminal a person can answer at.
    pub interactive: bool,
    /// The verb's `--timeout`, else [`DEFAULT_TIMEOUT`].
    pub timeout: Duration,
    /// How often a `wait_for` looks again.
    pub poll: Duration,
}

impl Options {
    /// `--no-prompt` and `--timeout` as given; the terminal is stdin's.
    #[must_use]
    pub fn from_verb(no_prompt: bool, timeout: Option<Duration>) -> Self {
        use std::io::IsTerminal;
        Self {
            no_prompt,
            interactive: std::io::stdin().is_terminal(),
            timeout: timeout.unwrap_or(DEFAULT_TIMEOUT),
            poll: Duration::from_millis(200),
        }
    }
}

/// The target's console, when a verb opened it before prepare.
pub type Console<'a> = &'a mut dyn BufRead;

/// Why prepare stopped. Every arm is exit 2 ([`Self::exit_code`]).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PrepareError {
    /// A `say` step with nobody to read it.
    #[error(
        "a prepare step needs a person, and there is no terminal (or `--no-prompt` was given): \
         {0}"
    )]
    NeedsPerson(String),
    /// A `wait_for` event did not happen in time.
    #[error("timed out after {secs} s waiting for `{event}`")]
    TimedOut { event: String, secs: u64 },
    /// A step that cannot be executed as written.
    #[error("{0}")]
    Step(String),
}

impl PrepareError {
    /// RFC-0118 D2: the runner could not do its work.
    #[must_use]
    pub fn exit_code(&self) -> u8 {
        nros_extension::exit::RUNNER_FAILED
    }
}

/// Run `steps` in order. Instructions go to `human` (the verb's stderr);
/// `console`, when given, feeds `line:` events.
///
/// # Errors
///
/// A [`PrepareError`]; nothing after the failing step runs.
pub fn execute(
    steps: &[PrepareStep],
    opts: &Options,
    host: &Host,
    human: &mut dyn Write,
    mut console: Option<Console<'_>>,
) -> Result<(), PrepareError> {
    for step in steps {
        match step {
            PrepareStep::Say(text) => {
                let _ = writeln!(human, "nros: {text}");
                if opts.no_prompt || !opts.interactive {
                    return Err(PrepareError::NeedsPerson(text.clone()));
                }
            }
            PrepareStep::WaitFor(raw) => {
                let event: WaitFor = raw.parse().map_err(|e: String| PrepareError::Step(e))?;
                let _ = writeln!(human, "nros: waiting for {event}");
                wait_for(&event, opts, host, console.as_mut())?;
            }
        }
    }
    Ok(())
}

fn wait_for(
    event: &WaitFor,
    opts: &Options,
    host: &Host,
    console: Option<&mut Console<'_>>,
) -> Result<(), PrepareError> {
    let start = Instant::now();
    let timed_out = || PrepareError::TimedOut {
        event: event.to_string(),
        secs: opts.timeout.as_secs(),
    };
    if let WaitFor::Line(text) = event {
        let Some(console) = console else {
            return Err(PrepareError::Step(format!(
                "`wait_for = \"line:{text}\"` waits on the console, and prepare runs before the \
                 target starts, so no console is open yet. Put the line in `--expect`, or use \
                 an event the host can see (`usb:`, `port:`, `volume:`)."
            )));
        };
        let mut line = String::new();
        while start.elapsed() < opts.timeout {
            line.clear();
            match console.read_line(&mut line) {
                Ok(0) => return Err(timed_out()),
                Ok(_) if line.contains(text.as_str()) => return Ok(()),
                Ok(_) => {}
                Err(e) => return Err(PrepareError::Step(format!("reading the console: {e}"))),
            }
        }
        return Err(timed_out());
    }
    loop {
        if holds(event, host) {
            return Ok(());
        }
        if start.elapsed() >= opts.timeout {
            return Err(timed_out());
        }
        thread::sleep(opts.poll);
    }
}

/// Whether a host-visible event holds now.
fn holds(event: &WaitFor, host: &Host) -> bool {
    match event {
        WaitFor::Usb(id) => usb_present(&host.sys, id.vid, id.pid),
        WaitFor::Port(p) => Path::new(p).exists(),
        WaitFor::Volume(label) => volume_mounted(host, label),
        WaitFor::Line(_) => false,
    }
}

/// A USB device with this VID:PID under `<sys>/bus/usb/devices`.
fn usb_present(sys: &Path, vid: u16, pid: u16) -> bool {
    let read_hex = |p: PathBuf| {
        std::fs::read_to_string(p)
            .ok()
            .and_then(|s| u16::from_str_radix(s.trim(), 16).ok())
    };
    let Ok(entries) = std::fs::read_dir(sys.join("bus/usb/devices")) else {
        return false;
    };
    entries.flatten().any(|e| {
        let d = e.path();
        read_hex(d.join("idVendor")) == Some(vid) && read_hex(d.join("idProduct")) == Some(pid)
    })
}

/// udev's by-label name for a label: a space is `\x20`, a slash `\x2f`.
fn by_label_name(label: &str) -> String {
    label
        .replace('\\', "\\x5c")
        .replace(' ', "\\x20")
        .replace('/', "\\x2f")
}

/// `<dev>/disk/by-label/<label>` exists and its device is a mount source in
/// `<proc>/mounts`.
fn volume_mounted(host: &Host, label: &str) -> bool {
    let link = host.dev.join("disk/by-label").join(by_label_name(label));
    let Ok(device) = std::fs::canonicalize(&link) else {
        return false;
    };
    let Ok(mounts) = std::fs::read_to_string(host.proc.join("mounts")) else {
        return false;
    };
    mounts.lines().any(|l| {
        let src = l.split_whitespace().next().unwrap_or("");
        src.starts_with('/') && std::fs::canonicalize(src).is_ok_and(|s| s == device)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(no_prompt: bool, interactive: bool) -> Options {
        Options {
            no_prompt,
            interactive,
            timeout: Duration::from_secs(5),
            poll: Duration::from_millis(20),
        }
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("nros-prepare-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// phase-529 W1 acceptance: a `say` / `wait_for = "port:<tmp>"` pair. The
    /// path appears after a delay and the executor continues; with
    /// `--no-prompt` it stops at the `say`, exit 2, the instruction on stderr.
    #[test]
    fn a_say_wait_for_port_pair_waits_then_continues_and_no_prompt_refuses() {
        let dir = tmp("port");
        let port = dir.join("ttyACM7");
        let steps = vec![
            PrepareStep::Say("Plug the board in".into()),
            PrepareStep::WaitFor(format!("port:{}", port.display())),
        ];
        let p = port.clone();
        let maker = thread::spawn(move || {
            thread::sleep(Duration::from_millis(300));
            std::fs::write(p, "").unwrap();
        });
        let started = Instant::now();
        let mut human = Vec::new();
        execute(
            &steps,
            &opts(false, true),
            &Host::under(&dir),
            &mut human,
            None,
        )
        .expect("the port appears and prepare continues");
        maker.join().unwrap();
        assert!(started.elapsed() >= Duration::from_millis(300), "it waited");
        let said = String::from_utf8(human).unwrap();
        assert!(
            said.contains("Plug the board in") && said.contains("waiting for port:"),
            "{said}"
        );

        // `--no-prompt`: exit 2, the instruction printed, nothing after it run.
        std::fs::remove_file(&port).unwrap();
        let mut human = Vec::new();
        let e = execute(
            &steps,
            &opts(true, true),
            &Host::under(&dir),
            &mut human,
            None,
        )
        .unwrap_err();
        assert_eq!(e.exit_code(), 2);
        assert!(matches!(&e, PrepareError::NeedsPerson(t) if t == "Plug the board in"));
        let said = String::from_utf8(human).unwrap();
        assert!(
            said.contains("Plug the board in") && !said.contains("waiting"),
            "{said}"
        );

        // No terminal is the same refusal.
        let e = execute(
            &steps,
            &opts(false, false),
            &Host::under(&dir),
            &mut Vec::new(),
            None,
        )
        .unwrap_err();
        assert!(matches!(e, PrepareError::NeedsPerson(_)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `--no-prompt` and `--timeout` reach the options; the terminal answer
    /// is stdin's (`IsTerminal`), which a test cannot fix either way.
    #[test]
    fn from_verb_reads_the_terminal_and_the_timeout() {
        let o = Options::from_verb(true, None);
        assert!(o.no_prompt);
        assert_eq!(o.timeout, DEFAULT_TIMEOUT);
        let o = Options::from_verb(false, Some(Duration::from_secs(7)));
        assert_eq!(o.timeout, Duration::from_secs(7));
    }

    #[test]
    fn a_usb_device_is_found_under_a_fake_sysfs() {
        let root = tmp("usb");
        let host = Host::under(&root);
        let steps = vec![PrepareStep::WaitFor("usb:0483:df11".into())];
        let quick = Options {
            timeout: Duration::from_millis(100),
            ..opts(false, true)
        };
        let e = execute(&steps, &quick, &host, &mut Vec::new(), None).unwrap_err();
        assert!(matches!(e, PrepareError::TimedOut { .. }), "{e}");
        let d = root.join("sys/bus/usb/devices/1-2");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("idVendor"), "0483\n").unwrap();
        std::fs::write(d.join("idProduct"), "df11\n").unwrap();
        execute(&steps, &quick, &host, &mut Vec::new(), None).expect("present now");
        // Another product of the same vendor does not answer.
        std::fs::write(d.join("idProduct"), "374b\n").unwrap();
        assert!(execute(&steps, &quick, &host, &mut Vec::new(), None).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn a_volume_counts_only_once_mounted() {
        let root = tmp("vol");
        let host = Host::under(&root);
        let quick = Options {
            timeout: Duration::from_millis(100),
            ..opts(false, true)
        };
        let steps = vec![PrepareStep::WaitFor("volume:RPI RP2".into())];
        let dev = root.join("dev");
        std::fs::create_dir_all(dev.join("disk/by-label")).unwrap();
        std::fs::write(dev.join("sdb1"), "").unwrap();
        std::os::unix::fs::symlink(dev.join("sdb1"), dev.join("disk/by-label/RPI\\x20RP2"))
            .unwrap();
        std::fs::create_dir_all(root.join("proc")).unwrap();
        std::fs::write(root.join("proc/mounts"), "proc /proc proc rw 0 0\n").unwrap();
        assert!(
            execute(&steps, &quick, &host, &mut Vec::new(), None).is_err(),
            "labelled but not mounted"
        );
        std::fs::write(
            root.join("proc/mounts"),
            format!(
                "{} /media/RPI-RP2 vfat rw 0 0\n",
                dev.join("sdb1").display()
            ),
        )
        .unwrap();
        execute(&steps, &quick, &host, &mut Vec::new(), None).expect("mounted");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_line_event_needs_a_console_and_reads_it_when_given() {
        let steps = vec![PrepareStep::WaitFor("line:bootloader ready".into())];
        let host = Host::under(Path::new("/nonexistent"));
        let e = execute(&steps, &opts(false, true), &host, &mut Vec::new(), None).unwrap_err();
        assert!(e.to_string().contains("no console is open"), "{e}");
        let mut console = std::io::Cursor::new(b"noise\nbootloader ready v2\n".to_vec());
        execute(
            &steps,
            &opts(false, true),
            &host,
            &mut Vec::new(),
            Some(&mut console),
        )
        .expect("the line arrives");
        let mut silent = std::io::Cursor::new(b"noise\n".to_vec());
        let e = execute(
            &steps,
            &opts(false, true),
            &host,
            &mut Vec::new(),
            Some(&mut silent),
        )
        .unwrap_err();
        assert!(matches!(e, PrepareError::TimedOut { .. }));
    }

    #[test]
    fn an_unknown_event_is_refused() {
        let steps = vec![PrepareStep::WaitFor("gpio:3".into())];
        let e = execute(
            &steps,
            &opts(false, true),
            &Host::under(Path::new("/x")),
            &mut Vec::new(),
            None,
        )
        .unwrap_err();
        assert!(e.to_string().contains("unknown event kind"), "{e}");
    }
}
