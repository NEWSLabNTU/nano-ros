//! A target session: the process group a verb starts, the console it captures,
//! the `--expect` grading over that console, and the teardown that stops all
//! of it (RFC-0118 D2 and D8, RFC-0119 D6).
//!
//! **Shared on purpose.** `nros run` is the first user. `nros flash --monitor`
//! and `nros monitor` reuse this module as it stands: they start a different
//! leader (a console runner instead of a launch runner), and everything else
//! here (grading, capture to `console.log`, the exit codes, stopping the whole
//! group on Ctrl-C, timeout, a match or verb exit) is the same for every verb,
//! which is why RFC-0119 D6 makes nros, not the runner, own it. Nothing in
//! this module knows which verb is calling it.
//!
//! # The pieces
//!
//! * [`ProcessGroup`]: every process the session starts (the runner, a router,
//!   an agent, a console command) joins ONE new process group, so one signal
//!   reaches the runner's own children too (QEMU under the qemu runner). On
//!   Linux each member also gets `PR_SET_PDEATHSIG`, so a verb killed with
//!   SIGKILL, which no handler sees, still takes its group with it.
//! * [`install_interrupt_handler`]: Ctrl-C, SIGTERM and SIGHUP set a flag the
//!   session loop polls, instead of killing the verb before it can stop the
//!   group. The group is not in the terminal's foreground, so a Ctrl-C reaches
//!   the verb alone, and the verb stops the group.
//! * [`spawn_console`]: tees a console stream to the verb's stdout and to
//!   `console.log`, unmodified, and grades [`Expect`] line by line (the
//!   unterminated tail of the stream counts as a line, so a prompt with no
//!   newline still matches).
//! * [`spawn_stderr`]: forwards a runner's stderr and reports its
//!   `nros-runner: link <endpoint>` line.
//! * [`Session::watch`]: the loop that decides the [`Outcome`], and
//!   [`Outcome::exit_code`], RFC-0118 D2's codes.

use std::{
    fs::File,
    io::{self, BufReader, Read, Write},
    process::{Child, Command, ExitStatus},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use nros_extension::exit;

/// How long a stopped group gets between SIGTERM and SIGKILL.
pub const TEARDOWN_GRACE: Duration = Duration::from_secs(2);

/// Exit code for a session the user interrupted: the shell's convention for
/// SIGINT (128 + 2). Not one of RFC-0118 D2's verdicts, because an interrupt
/// is a person stopping the verb, not an answer about the target.
pub const INTERRUPTED: u8 = 130;

// ── Ctrl-C ──────────────────────────────────────────────────────────────

static INTERRUPT: AtomicBool = AtomicBool::new(false);

#[cfg(unix)]
extern "C" fn on_signal(_sig: libc::c_int) {
    // Only an atomic store: the one thing a signal handler may safely do.
    INTERRUPT.store(true, Ordering::SeqCst);
}

/// Route SIGINT, SIGTERM and SIGHUP to a flag [`Session::watch`] polls, so the
/// verb lives long enough to stop its process group. Idempotent.
pub fn install_interrupt_handler() {
    #[cfg(unix)]
    {
        let handler = on_signal as extern "C" fn(libc::c_int) as libc::sighandler_t;
        for sig in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP] {
            // SAFETY: installs an async-signal-safe handler (one atomic store).
            unsafe {
                libc::signal(sig, handler);
            }
        }
    }
}

/// Whether an interrupt arrived since [`install_interrupt_handler`].
#[must_use]
pub fn interrupted() -> bool {
    INTERRUPT.load(Ordering::SeqCst)
}

/// Raise the interrupt flag as a signal would. For tests and for a caller
/// that decides to stop the session from another thread.
pub fn raise_interrupt() {
    INTERRUPT.store(true, Ordering::SeqCst);
}

/// Lower the flag (a test that raised it, before the next test runs).
pub fn clear_interrupt() {
    INTERRUPT.store(false, Ordering::SeqCst);
}

// ── The process group ───────────────────────────────────────────────────

/// One member of a [`ProcessGroup`].
#[derive(Debug)]
pub struct Member {
    /// The caller's name for it (`runner`, `router`, `agent`).
    pub name: String,
    /// The process.
    pub child: Child,
    /// Its exit status, once reaped.
    pub status: Option<ExitStatus>,
}

/// Every process a session starts, in one new process group.
///
/// The first member spawned creates the group; every later one joins it.
/// Dropping the group stops it ([`ProcessGroup::teardown`]).
#[derive(Debug, Default)]
pub struct ProcessGroup {
    pgid: Option<i32>,
    members: Vec<Member>,
}

impl ProcessGroup {
    /// An empty group; nothing runs yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The group id, once a member exists.
    #[must_use]
    pub fn pgid(&self) -> Option<i32> {
        self.pgid
    }

    /// Spawn `cmd` into the group under `name`.
    ///
    /// # Errors
    ///
    /// The spawn failed.
    pub fn spawn(&mut self, name: &str, mut cmd: Command) -> io::Result<&mut Member> {
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(self.pgid.unwrap_or(0));
            #[cfg(target_os = "linux")]
            // SAFETY: `prctl` is async-signal-safe and touches only the child.
            unsafe {
                cmd.pre_exec(|| {
                    if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
                        return Err(io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
        }
        let child = cmd.spawn()?;
        if self.pgid.is_none() {
            self.pgid = i32::try_from(child.id()).ok();
        }
        self.members.push(Member {
            name: name.to_string(),
            child,
            status: None,
        });
        Ok(self.members.last_mut().expect("just pushed"))
    }

    /// The member called `name`.
    pub fn member(&mut self, name: &str) -> Option<&mut Member> {
        self.members.iter_mut().find(|m| m.name == name)
    }

    /// Reap `name` if it has exited, and return its status.
    pub fn poll(&mut self, name: &str) -> Option<ExitStatus> {
        let m = self.member(name)?;
        if m.status.is_none()
            && let Ok(Some(s)) = m.child.try_wait()
        {
            m.status = Some(s);
        }
        m.status
    }

    /// Stop the whole group: SIGTERM, up to `grace` for it to go, then
    /// SIGKILL; then reap every member. Safe to call twice.
    pub fn teardown(&mut self, grace: Duration) {
        #[cfg(unix)]
        if let Some(pgid) = self.pgid {
            signal_group(pgid, libc::SIGTERM);
            let deadline = Instant::now() + grace;
            while Instant::now() < deadline && !group_members(pgid).is_empty() {
                self.reap();
                thread::sleep(Duration::from_millis(50));
            }
            if !group_members(pgid).is_empty() {
                signal_group(pgid, libc::SIGKILL);
            }
        }
        for m in &mut self.members {
            if m.status.is_none() {
                let _ = m.child.kill();
                m.status = m.child.wait().ok();
            }
        }
    }

    fn reap(&mut self) {
        for m in &mut self.members {
            if m.status.is_none()
                && let Ok(Some(s)) = m.child.try_wait()
            {
                m.status = Some(s);
            }
        }
    }
}

impl Drop for ProcessGroup {
    fn drop(&mut self) {
        self.teardown(TEARDOWN_GRACE);
    }
}

#[cfg(unix)]
fn signal_group(pgid: i32, sig: libc::c_int) {
    // SAFETY: plain syscall; a group that is already gone answers ESRCH.
    unsafe {
        libc::kill(-pgid, sig);
    }
}

/// The live (not zombie) processes in process group `pgid`, read from
/// `/proc`. Empty where `/proc` does not exist. The teardown check, and what
/// a test asserts after Ctrl-C.
#[must_use]
pub fn group_members(pgid: i32) -> Vec<i32> {
    let Ok(dir) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for e in dir.flatten() {
        let Some(pid) = e.file_name().to_str().and_then(|s| s.parse::<i32>().ok()) else {
            continue;
        };
        let Ok(stat) = std::fs::read_to_string(e.path().join("stat")) else {
            continue;
        };
        // `pid (comm) state ppid pgrp ...`; comm may hold spaces and parens,
        // so parse from the LAST `)`.
        let Some(rest) = stat.rsplit_once(')').map(|(_, r)| r) else {
            continue;
        };
        let mut f = rest.split_whitespace();
        let state = f.next();
        let _ppid = f.next();
        let pgrp = f.next().and_then(|s| s.parse::<i32>().ok());
        if pgrp == Some(pgid) && state != Some("Z") && state != Some("X") {
            out.push(pid);
        }
    }
    out
}

// ── Grading ─────────────────────────────────────────────────────────────

/// What `--expect` / `--expect-regex` wait for on the console.
#[derive(Debug, Clone)]
pub enum Expect {
    /// A substring of one console line.
    Substring(String),
    /// A regular expression over one console line.
    Regex(regex::Regex),
}

impl Expect {
    /// `--expect <text>`.
    #[must_use]
    pub fn substring(text: &str) -> Self {
        Expect::Substring(text.to_string())
    }

    /// `--expect-regex <re>`.
    ///
    /// # Errors
    ///
    /// The expression does not compile.
    pub fn regex(re: &str) -> Result<Self, regex::Error> {
        regex::Regex::new(re).map(Expect::Regex)
    }

    /// Whether `line` (without its line ending) satisfies it.
    #[must_use]
    pub fn matches(&self, line: &str) -> bool {
        match self {
            Expect::Substring(s) => line.contains(s.as_str()),
            Expect::Regex(r) => r.is_match(line),
        }
    }

    /// The pattern as the user wrote it.
    #[must_use]
    pub fn pattern(&self) -> &str {
        match self {
            Expect::Substring(s) => s,
            Expect::Regex(r) => r.as_str(),
        }
    }
}

// ── Capture ─────────────────────────────────────────────────────────────

/// What the capture threads report to [`Session::watch`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// The expectation matched this console line.
    Matched(String),
    /// The console stream ended.
    ConsoleClosed,
    /// A runner reported its link endpoint on stderr.
    Link(String),
}

/// Tee `from` (a console stream) to `echo` and `log`, byte for byte, and grade
/// `expect` over its lines. Sends [`Event::Matched`] once, at the first
/// matching line, and [`Event::ConsoleClosed`] at end of stream.
pub fn spawn_console<R>(
    from: R,
    mut echo: Box<dyn Write + Send>,
    mut log: Option<File>,
    expect: Option<Expect>,
    tx: Sender<Event>,
) -> JoinHandle<()>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut from = from;
        let mut buf = [0u8; 8192];
        let mut line: Vec<u8> = Vec::new();
        let mut pending = expect;
        loop {
            let n = match from.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => break,
            };
            let chunk = &buf[..n];
            // The verb's stdout may be a closed pipe (`| head`): keep capturing
            // and grading anyway, the log and the verdict still matter.
            let _ = echo.write_all(chunk).and_then(|()| echo.flush());
            if let Some(f) = log.as_mut() {
                let _ = f.write_all(chunk).and_then(|()| f.flush());
            }
            let Some(exp) = pending.as_ref() else {
                continue;
            };
            for &b in chunk {
                if b == b'\n' {
                    if grade(exp, &line, &tx) {
                        pending = None;
                        line.clear();
                        break;
                    }
                    line.clear();
                } else {
                    line.push(b);
                }
            }
            // The unterminated tail is a line too (a prompt, a last line
            // whose newline has not been written yet).
            if let Some(exp) = pending.as_ref()
                && !line.is_empty()
                && grade(exp, &line, &tx)
            {
                pending = None;
                line.clear();
            }
        }
        let _ = tx.send(Event::ConsoleClosed);
    })
}

fn grade(exp: &Expect, line: &[u8], tx: &Sender<Event>) -> bool {
    let text = String::from_utf8_lossy(line);
    let text = text.trim_end_matches('\r');
    if exp.matches(text) {
        let _ = tx.send(Event::Matched(text.to_string()));
        return true;
    }
    false
}

/// Forward a runner's stderr to the verb's stderr and report its link line
/// (RFC-0119 D6) as [`Event::Link`].
pub fn spawn_stderr<R>(from: R, tx: Sender<Event>) -> JoinHandle<()>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let _ = nros_extension::console::scan_stderr(BufReader::new(from), io::stderr(), |ep| {
            let _ = tx.send(Event::Link(ep.to_string()));
        });
    })
}

/// A channel for the capture threads.
#[must_use]
pub fn channel() -> (Sender<Event>, Receiver<Event>) {
    mpsc::channel()
}

// ── The session loop ────────────────────────────────────────────────────

/// How a session ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The expectation matched this line.
    Matched(String),
    /// The leader exited with this code (`None`: killed by a signal).
    Exited(Option<i32>),
    /// `--timeout` passed.
    TimedOut,
    /// Ctrl-C, SIGTERM or SIGHUP.
    Interrupted,
    /// A member the session needs (a router, an agent) exited first.
    MemberDied {
        /// Its name.
        name: String,
        /// Its exit code.
        code: Option<i32>,
    },
    /// Something the session itself had to do failed (a router that could
    /// not start once the link was known).
    Failed(String),
}

impl Outcome {
    /// RFC-0118 D2: 0 success or matched, 1 the target failed or the
    /// expectation timed out, 2 the runner failed. `expecting` is whether an
    /// expectation was given. An interrupt is [`INTERRUPTED`].
    #[must_use]
    pub fn exit_code(&self, expecting: bool) -> u8 {
        match self {
            Outcome::Matched(_) => exit::SUCCESS,
            Outcome::TimedOut if expecting => exit::TARGET_FAILED,
            Outcome::TimedOut => exit::SUCCESS,
            Outcome::Exited(code) => {
                let runner = nros_extension::exit::RunnerExit::from_code(*code);
                match runner {
                    // The target ended and never printed what was expected.
                    nros_extension::exit::RunnerExit::Success if expecting => exit::TARGET_FAILED,
                    r => r.code(),
                }
            }
            Outcome::Interrupted => INTERRUPTED,
            Outcome::MemberDied { .. } | Outcome::Failed(_) => exit::RUNNER_FAILED,
        }
    }

    /// One line for the verb's stderr.
    #[must_use]
    pub fn describe(&self, expect: Option<&Expect>, elapsed: Duration) -> String {
        let t = elapsed.as_secs_f64();
        match self {
            Outcome::Matched(line) => format!("matched after {t:.1} s: {line}"),
            Outcome::TimedOut => match expect {
                Some(e) => format!("timed out after {t:.1} s without `{}`", e.pattern()),
                None => format!("stopped after {t:.1} s (--timeout)"),
            },
            Outcome::Exited(code) => {
                let how = code.map_or_else(
                    || "was killed by a signal".to_string(),
                    |c| format!("exited with code {c}"),
                );
                match expect {
                    Some(e) => format!("the target {how} after {t:.1} s without `{}`", e.pattern()),
                    None => format!("the target {how} after {t:.1} s"),
                }
            }
            Outcome::Interrupted => format!("interrupted after {t:.1} s"),
            Outcome::MemberDied { name, code } => format!(
                "the {name} exited ({}) after {t:.1} s, so the target cannot work",
                code.map_or_else(|| "signal".to_string(), |c| format!("code {c}"))
            ),
            Outcome::Failed(why) => why.clone(),
        }
    }
}

/// The running session: its group, its leader and what it must keep alive.
pub struct Session {
    /// The group every process is in.
    pub group: ProcessGroup,
    /// The member whose exit ends the session (the runner).
    pub leader: String,
    /// Members whose early exit ends the session as a runner failure.
    pub required: Vec<String>,
    /// `--timeout`.
    pub timeout: Option<Duration>,
    /// How long the console may keep draining after the leader exited.
    pub drain: Duration,
}

impl Session {
    /// A session over `group`, ended by `leader`.
    #[must_use]
    pub fn new(group: ProcessGroup, leader: &str, timeout: Option<Duration>) -> Self {
        Self {
            group,
            leader: leader.to_string(),
            required: Vec::new(),
            timeout,
            drain: Duration::from_secs(2),
        }
    }

    /// Wait for the outcome. `on_link` is called with each reported link
    /// endpoint and may start members (a serial router dials the pty the
    /// runner reported); its error ends the session as [`Outcome::Failed`].
    /// The group is left running: the caller stops it with
    /// [`ProcessGroup::teardown`] (or by dropping the session).
    pub fn watch(
        &mut self,
        rx: &Receiver<Event>,
        started: Instant,
        mut on_link: impl FnMut(&str, &mut Session) -> Result<(), String>,
    ) -> Outcome {
        let deadline = self.timeout.map(|t| started + t);
        let mut console_closed = false;
        let mut leader_exit: Option<(Option<i32>, Instant)> = None;
        loop {
            if interrupted() {
                return Outcome::Interrupted;
            }
            if deadline.is_some_and(|d| Instant::now() >= d) {
                return Outcome::TimedOut;
            }
            match rx.recv_timeout(Duration::from_millis(50)) {
                Ok(Event::Matched(line)) => return Outcome::Matched(line),
                Ok(Event::ConsoleClosed) => console_closed = true,
                Ok(Event::Link(ep)) => {
                    if let Err(e) = on_link(&ep, self) {
                        return Outcome::Failed(e);
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => console_closed = true,
            }
            for name in self.required.clone() {
                if let Some(s) = self.group.poll(&name) {
                    return Outcome::MemberDied {
                        name,
                        code: s.code(),
                    };
                }
            }
            if leader_exit.is_none() {
                let leader = self.leader.clone();
                if let Some(s) = self.group.poll(&leader) {
                    leader_exit = Some((s.code(), Instant::now()));
                }
            }
            // The leader is gone: give the console its last bytes (a match
            // in the final line still counts), then report the exit.
            if let Some((code, at)) = leader_exit
                && (console_closed || at.elapsed() >= self.drain)
            {
                // One more look for a match that raced the close.
                while let Ok(ev) = rx.try_recv() {
                    if let Event::Matched(line) = ev {
                        return Outcome::Matched(line);
                    }
                }
                return Outcome::Exited(code);
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::process::Stdio;

    // The interrupt flag is process-global, so the tests touching it share
    // one lock.
    pub(crate) static FLAG: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn sh(script: &str) -> Command {
        let mut c = Command::new("sh");
        c.arg("-c").arg(script);
        c.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        c
    }

    fn start(
        script: &str,
        expect: Option<Expect>,
    ) -> (Session, Receiver<Event>, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let mut group = ProcessGroup::new();
        let m = group.spawn("runner", sh(script)).unwrap();
        let out = m.child.stdout.take().unwrap();
        let err = m.child.stderr.take().unwrap();
        let (tx, rx) = channel();
        let log = File::create(dir.path().join("console.log")).unwrap();
        spawn_console(out, Box::new(io::sink()), Some(log), expect, tx.clone());
        spawn_stderr(err, tx);
        (
            Session::new(group, "runner", Some(Duration::from_secs(10))),
            rx,
            dir,
        )
    }

    #[test]
    fn a_substring_match_ends_the_session_with_zero() {
        let _g = FLAG
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let (mut s, rx, dir) = start(
            "echo booting; echo 'Publishing: 0'; sleep 30",
            Some(Expect::substring("Publishing: 0")),
        );
        let o = s.watch(&rx, Instant::now(), |_, _| Ok(()));
        assert_eq!(o, Outcome::Matched("Publishing: 0".into()));
        assert_eq!(o.exit_code(true), 0);
        let pgid = s.group.pgid().unwrap();
        s.group.teardown(TEARDOWN_GRACE);
        assert!(group_members(pgid).is_empty());
        let log = std::fs::read_to_string(dir.path().join("console.log")).unwrap();
        assert!(log.starts_with("booting\nPublishing: 0\n"), "{log}");
    }

    #[test]
    fn a_regex_matches_an_unterminated_tail() {
        let _g = FLAG
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let (mut s, rx, _d) = start(
            "printf 'count=42'; sleep 30",
            Some(Expect::regex(r"count=\d+$").unwrap()),
        );
        let o = s.watch(&rx, Instant::now(), |_, _| Ok(()));
        assert_eq!(o, Outcome::Matched("count=42".into()));
    }

    #[test]
    fn exit_without_the_match_is_a_target_failure() {
        let _g = FLAG
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let (mut s, rx, _d) = start("echo nothing here", Some(Expect::substring("never")));
        let o = s.watch(&rx, Instant::now(), |_, _| Ok(()));
        assert_eq!(o, Outcome::Exited(Some(0)));
        assert_eq!(o.exit_code(true), 1);
        assert_eq!(o.exit_code(false), 0);
    }

    #[test]
    fn a_runner_failure_is_exit_two() {
        assert_eq!(Outcome::Exited(Some(2)).exit_code(false), 2);
        assert_eq!(Outcome::Exited(Some(2)).exit_code(true), 2);
        assert_eq!(Outcome::Exited(Some(1)).exit_code(false), 1);
        assert_eq!(Outcome::Exited(None).exit_code(false), 2);
        assert_eq!(Outcome::Failed("x".into()).exit_code(true), 2);
    }

    #[test]
    fn the_timeout_is_a_failure_only_when_expecting() {
        let _g = FLAG
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let (mut s, rx, _d) = start("sleep 30", Some(Expect::substring("x")));
        s.timeout = Some(Duration::from_millis(300));
        let o = s.watch(&rx, Instant::now(), |_, _| Ok(()));
        assert_eq!(o, Outcome::TimedOut);
        assert_eq!(o.exit_code(true), 1);
        assert_eq!(o.exit_code(false), 0);
    }

    #[test]
    fn the_link_line_reaches_the_callback() {
        let _g = FLAG
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let (mut s, rx, _d) = start(
            "echo 'nros-runner: link /dev/pts/9' >&2; sleep 30",
            Some(Expect::substring("never")),
        );
        s.timeout = Some(Duration::from_secs(5));
        let mut seen = Vec::new();
        let o = s.watch(&rx, Instant::now(), |ep, _| {
            seen.push(ep.to_string());
            Err("stop".into())
        });
        assert_eq!(seen, ["/dev/pts/9"]);
        assert_eq!(o, Outcome::Failed("stop".into()));
    }

    #[test]
    fn interrupt_stops_the_whole_group_grandchildren_included() {
        let _g = FLAG
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // The runner starts a grandchild of its own (QEMU under a runner) and
        // a second member joins the group (a router).
        let (mut s, rx, _d) = start("sleep 300 & sleep 300 & wait", None);
        s.group
            .spawn("router", {
                let mut c = Command::new("sleep");
                c.arg("300");
                c
            })
            .unwrap();
        s.required.push("router".into());
        let pgid = s.group.pgid().unwrap();
        thread::sleep(Duration::from_millis(200));
        assert!(group_members(pgid).len() >= 4, "{:?}", group_members(pgid));
        raise_interrupt();
        let o = s.watch(&rx, Instant::now(), |_, _| Ok(()));
        clear_interrupt();
        assert_eq!(o, Outcome::Interrupted);
        assert_eq!(o.exit_code(false), INTERRUPTED);
        s.group.teardown(TEARDOWN_GRACE);
        assert!(
            group_members(pgid).is_empty(),
            "left behind: {:?}",
            group_members(pgid)
        );
    }

    #[test]
    fn a_required_member_dying_is_a_runner_failure() {
        let _g = FLAG
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let (mut s, rx, _d) = start("sleep 30", None);
        s.group
            .spawn("router", {
                let mut c = Command::new("sh");
                c.arg("-c").arg("exit 7");
                c
            })
            .unwrap();
        s.required.push("router".into());
        let o = s.watch(&rx, Instant::now(), |_, _| Ok(()));
        assert_eq!(
            o,
            Outcome::MemberDied {
                name: "router".into(),
                code: Some(7)
            }
        );
        assert_eq!(o.exit_code(false), 2);
    }
}
