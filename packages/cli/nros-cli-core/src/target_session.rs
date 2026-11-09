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
//!   newline still matches; a match there waits up to [`LINE_FINISH`] for
//!   the rest of that line, so a line written in pieces is reported whole).
//! * [`spawn_stderr`]: forwards a runner's stderr, reports its
//!   `nros-runner: link <endpoint>` line, and records every
//!   `nros-runner: chain <pgid>` line in the group's [`ChainLog`], so the
//!   teardown also waits for a runner's process chains (issue 2307).
//! * [`Session::watch`]: the loop that decides the [`Outcome`], and
//!   [`Outcome::exit_code`], RFC-0118 D2's codes.

use std::{
    fs::File,
    io::{self, BufReader, Read, Write},
    process::{Child, Command, ExitStatus},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use nros_extension::exit;

/// How long the teardown waits for a runner's process chains to stop through
/// their own guard (issue 2307). The guard gives its group one second between
/// SIGTERM and SIGKILL, so a chain that is still there after this is stuck:
/// the verb SIGKILLs it and says so.
pub const CHAIN_GRACE: Duration = Duration::from_millis(2500);

/// How long a stopped group gets between SIGTERM and SIGKILL.
pub const TEARDOWN_GRACE: Duration = Duration::from_secs(2);

/// How long a match in an unterminated line waits for the rest of that line
/// before the session ends on it (phase-548 W10). A target may write one line
/// in pieces (the NuttX C++ talker does), so a match on the first piece would
/// otherwise stop the emulator mid-line and report half a line.
pub const LINE_FINISH: Duration = Duration::from_millis(500);

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
    chains: ChainLog,
}

/// The process groups a runner's chains lead (`nros-runner: chain <pgid>`).
/// A chain is a group of its own, outside the session's, so stopping the
/// session's group does not stop it; the teardown waits for these too.
#[derive(Debug, Default, Clone)]
pub struct ChainLog(Arc<Mutex<Vec<i32>>>);

impl ChainLog {
    /// Record a chain's group.
    pub fn record(&self, pgid: u32) {
        let Ok(pgid) = i32::try_from(pgid) else {
            return;
        };
        let mut v = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !v.contains(&pgid) {
            v.push(pgid);
        }
    }

    fn take(&self) -> Vec<i32> {
        std::mem::take(
            &mut *self
                .0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        )
    }
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
            // PDEATHSIG watches the forking THREAD; every member is spawned
            // from the verb's main thread, which lives as long as the verb.
            // A verb that died between fork and prctl has already reparented
            // the child, so getppid() is re-read after prctl (issue 2307; the
            // runners' twin is `nros_runner_lifetime::die_with_runner`).
            {
                let verb = std::process::id() as libc::pid_t;
                // SAFETY: `prctl` and `getppid` are async-signal-safe and
                // touch only the child.
                unsafe {
                    cmd.pre_exec(move || {
                        if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
                            return Err(io::Error::last_os_error());
                        }
                        if libc::getppid() != verb {
                            return Err(io::Error::from_raw_os_error(libc::ESRCH));
                        }
                        Ok(())
                    });
                }
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

    /// Where a runner's stderr scan records its chains ([`spawn_stderr`]).
    #[must_use]
    pub fn chain_log(&self) -> ChainLog {
        self.chains.clone()
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
        // The runner is gone now, so each chain's guard has seen its pipe
        // close and is stopping the chain. Wait for that, so the verb does
        // not return while the chain still holds a port or a device.
        for pgid in stop_chains(&self.chains.take(), CHAIN_GRACE) {
            eprintln!(
                "nros: process chain {pgid} was still running {:.1} s after its runner \
                 stopped; killed it (SIGKILL)",
                CHAIN_GRACE.as_secs_f32()
            );
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

/// Wait up to `grace` for every group in `chains` to empty, then SIGKILL the
/// ones that have not, and return those. A chain's guard leads its group, so
/// an empty group cannot be a reused id: the id stays taken while the guard
/// lives.
fn stop_chains(chains: &[i32], grace: Duration) -> Vec<i32> {
    let deadline = Instant::now() + grace;
    let mut left: Vec<i32> = chains.to_vec();
    loop {
        left.retain(|&g| !group_members(g).is_empty());
        if left.is_empty() || Instant::now() >= deadline {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    #[cfg(unix)]
    for &g in &left {
        signal_group(g, libc::SIGKILL);
    }
    left
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
    // A group with no process left at all (not even a zombie) answers ESRCH,
    // and that is the common case at teardown: one syscall instead of a stat
    // read per process on the host (phase-528 W5 measured ~8k syscalls per
    // `nros run` exit, twice, on a host running ~2,000 processes).
    #[cfg(unix)]
    // SAFETY: signal 0 delivers nothing; it only asks whether the group exists.
    if unsafe { libc::kill(-pgid, 0) } != 0
        && io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
    {
        return Vec::new();
    }
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
    /// The expectation matched an unterminated line, which is this so far.
    /// The capture keeps reading to the end of it and then sends
    /// [`Event::Matched`] with the whole line; [`Session::watch`] waits for
    /// that at most [`LINE_FINISH`], and ends on the longest piece it has.
    Matching(String),
    /// The console stream ended.
    ConsoleClosed,
    /// A runner reported its link endpoint on stderr.
    Link(String),
}

/// Tee `from` (a console stream) to `echo` and `log`, byte for byte, and grade
/// `expect` over its lines. Sends [`Event::Matched`] once, with the first
/// matching line, and [`Event::ConsoleClosed`] at end of stream.
///
/// A match in the unterminated tail is not the end of that line: the tail is
/// reported as [`Event::Matching`] (again each time it grows) and grading
/// becomes "finish this line", so [`Event::Matched`] carries the whole line
/// once its newline arrives, or the tail as it stood at end of stream.
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
        let mut grading = match expect {
            Some(e) => Grading::Looking(e),
            None => Grading::Done,
        };
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
            grading.feed(chunk, &mut line, &tx);
        }
        if let Grading::Finishing = grading {
            let _ = tx.send(Event::Matched(line_text(&line)));
        }
        let _ = tx.send(Event::ConsoleClosed);
    })
}

/// Where [`spawn_console`]'s grading stands.
enum Grading {
    /// No line has matched yet.
    Looking(Expect),
    /// The unterminated line in the buffer matched; waiting for its newline.
    Finishing,
    /// [`Event::Matched`] was sent, or there was nothing to grade.
    Done,
}

impl Grading {
    /// Grade one chunk of console bytes, `line` carrying the unterminated
    /// line between chunks.
    fn feed(&mut self, chunk: &[u8], line: &mut Vec<u8>, tx: &Sender<Event>) {
        if let Grading::Done = self {
            return;
        }
        for &b in chunk {
            if b != b'\n' {
                line.push(b);
                continue;
            }
            let ended = match self {
                Grading::Looking(exp) => exp.matches(&line_text(line)),
                Grading::Finishing => true,
                Grading::Done => unreachable!("returned above"),
            };
            if ended {
                let _ = tx.send(Event::Matched(line_text(line)));
                *self = Grading::Done;
                line.clear();
                return;
            }
            line.clear();
        }
        // The unterminated tail is a line too (a prompt, a last line whose
        // newline has not been written yet, a line written in pieces).
        if line.is_empty() {
            return;
        }
        let text = line_text(line);
        match self {
            Grading::Looking(exp) if exp.matches(&text) => {
                let _ = tx.send(Event::Matching(text));
                *self = Grading::Finishing;
            }
            Grading::Finishing => {
                let _ = tx.send(Event::Matching(text));
            }
            _ => {}
        }
    }
}

/// One console line as text, without its line ending.
fn line_text(line: &[u8]) -> String {
    String::from_utf8_lossy(line)
        .trim_end_matches('\r')
        .to_string()
}

/// Forward a runner's stderr to the verb's stderr, report its link line
/// (RFC-0119 D6) as [`Event::Link`], and record its chain lines in `chains`
/// ([`ProcessGroup::chain_log`]).
pub fn spawn_stderr<R>(from: R, tx: Sender<Event>, chains: ChainLog) -> JoinHandle<()>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let _ = nros_extension::console::scan_stderr_with(
            BufReader::new(from),
            io::stderr(),
            |ep| {
                let _ = tx.send(Event::Link(ep.to_string()));
            },
            |pgid| chains.record(pgid),
        );
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
        // A match in a line still being written: the line so far, and when
        // it matched. Once anything matched the verdict is a match; what is
        // left to decide is only how much of the line it reports.
        let mut finishing: Option<(String, Instant)> = None;
        loop {
            if interrupted() {
                return Outcome::Interrupted;
            }
            if let Some((line, at)) = &finishing
                && (at.elapsed() >= LINE_FINISH || console_closed)
            {
                return Outcome::Matched(line.clone());
            }
            if deadline.is_some_and(|d| Instant::now() >= d) {
                return match finishing {
                    Some((line, _)) => Outcome::Matched(line),
                    None => Outcome::TimedOut,
                };
            }
            match rx.recv_timeout(Duration::from_millis(50)) {
                Ok(Event::Matched(line)) => return Outcome::Matched(line),
                Ok(Event::Matching(line)) => {
                    let at = finishing.map_or_else(Instant::now, |(_, at)| at);
                    finishing = Some((line, at));
                    continue;
                }
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
                    if let Some((line, _)) = finishing {
                        return Outcome::Matched(line);
                    }
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
                    match ev {
                        Event::Matched(line) => return Outcome::Matched(line),
                        Event::Matching(line) => {
                            let at = finishing.map_or_else(Instant::now, |(_, at)| at);
                            finishing = Some((line, at));
                        }
                        _ => {}
                    }
                }
                if let Some((line, _)) = finishing {
                    return Outcome::Matched(line);
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
        let chains = group.chain_log();
        spawn_stderr(err, tx, chains);
        (
            Session::new(group, "runner", Some(Duration::from_secs(10))),
            rx,
            dir,
        )
    }

    /// A runner that starts `chain` as a group of its own (`setsid`, as the
    /// lifetime guard leads its own) and names it with a chain line, then
    /// waits. Returns the group and the chain's group id.
    #[cfg(target_os = "linux")]
    fn runner_with_chain(chain: &str) -> (ProcessGroup, i32) {
        let mut group = ProcessGroup::new();
        let script = format!(
            "setsid {chain} </dev/null >/dev/null 2>&1 & \
             echo \"{}$!\" >&2; wait",
            nros_extension::console::CHAIN_PREFIX
        );
        let m = group.spawn("runner", sh(&script)).unwrap();
        let err = m.child.stderr.take().unwrap();
        let _ = m.child.stdout.take();
        let (tx, _rx) = channel();
        let chains = group.chain_log();
        spawn_stderr(err, tx, chains.clone());
        let deadline = Instant::now() + Duration::from_secs(5);
        let pgid = loop {
            if let Some(&g) = chains.0.lock().unwrap().first() {
                break g;
            }
            assert!(Instant::now() < deadline, "no chain line");
            thread::sleep(Duration::from_millis(10));
        };
        // `setsid` makes its pid the group id once it has run.
        while group_members(pgid).is_empty() {
            assert!(Instant::now() < deadline, "the chain never started");
            thread::sleep(Duration::from_millis(10));
        }
        (group, pgid)
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn teardown_waits_for_a_chain_that_is_stopping() {
        // The chain outlives its runner by about half a second, as a chain
        // does while its guard goes from SIGTERM to SIGKILL.
        // It leaves a mark when it ends on its own, so a chain the teardown
        // killed early instead of waiting for is told apart.
        let dir = tempfile::tempdir().unwrap();
        let mark = dir.path().join("ended");
        let (mut group, pgid) =
            runner_with_chain(&format!("sh -c 'sleep 0.5; touch {}'", mark.display()));
        let t = Instant::now();
        group.teardown(TEARDOWN_GRACE);
        assert!(group_members(pgid).is_empty(), "teardown returned first");
        assert!(
            mark.exists(),
            "the teardown killed a chain that was stopping"
        );
        assert!(
            t.elapsed() < CHAIN_GRACE,
            "a chain that stops by itself is not killed: {:?}",
            t.elapsed()
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn teardown_kills_a_chain_that_does_not_stop() {
        let (mut group, pgid) = runner_with_chain("sleep 30");
        let t = Instant::now();
        group.teardown(TEARDOWN_GRACE);
        let waited = t.elapsed();
        let deadline = Instant::now() + Duration::from_secs(2);
        while !group_members(pgid).is_empty() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        let left = group_members(pgid);
        // Never leave the survivor behind a red test.
        signal_group(pgid, libc::SIGKILL);
        assert!(left.is_empty(), "chain {pgid} survived: {left:?}");
        assert!(waited >= CHAIN_GRACE, "killed before the grace: {waited:?}");
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
    fn a_line_written_in_two_pieces_is_reported_and_logged_whole() {
        let _g = FLAG
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // The NuttX C++ talker's shape: the match is in the first piece.
        let (mut s, rx, dir) = start(
            "printf \"Publishing: 'Hello\"; sleep 0.2; printf \" World: 1'\\n\"; sleep 30",
            Some(Expect::substring("Publishing:")),
        );
        let o = s.watch(&rx, Instant::now(), |_, _| Ok(()));
        assert_eq!(o, Outcome::Matched("Publishing: 'Hello World: 1'".into()));
        // Logged (and echoed: the tee writes both before grading) while the
        // target still runs, so stopping it now cuts nothing.
        let log = std::fs::read_to_string(dir.path().join("console.log")).unwrap();
        assert_eq!(log, "Publishing: 'Hello World: 1'\n");
        let pgid = s.group.pgid().unwrap();
        assert!(!group_members(pgid).is_empty(), "stopped before the match");
        s.group.teardown(TEARDOWN_GRACE);
    }

    #[test]
    fn a_line_that_never_finishes_is_reported_after_the_bound() {
        let _g = FLAG
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let (mut s, rx, _d) = start(
            "printf 'Publishing: x'; sleep 0.1; printf 'y'; sleep 30",
            Some(Expect::substring("Publishing:")),
        );
        let t = Instant::now();
        let o = s.watch(&rx, t, |_, _| Ok(()));
        assert_eq!(o, Outcome::Matched("Publishing: xy".into()));
        assert!(t.elapsed() >= LINE_FINISH, "{:?}", t.elapsed());
        assert!(t.elapsed() < Duration::from_secs(5), "{:?}", t.elapsed());
    }

    #[test]
    fn grading_finishes_the_matched_line_across_chunks() {
        let (tx, rx) = channel();
        let mut g = Grading::Looking(Expect::substring("Pub"));
        let mut line = Vec::new();
        g.feed(b"boot\nPub", &mut line, &tx);
        g.feed(b"lishing: 'Hel", &mut line, &tx);
        g.feed(b"lo'\r\nnext\n", &mut line, &tx);
        drop(tx);
        let got: Vec<Event> = rx.iter().collect();
        assert_eq!(
            got,
            [
                Event::Matching("Pub".into()),
                Event::Matching("Publishing: 'Hel".into()),
                Event::Matched("Publishing: 'Hello'".into()),
            ]
        );
        assert!(matches!(g, Grading::Done));
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
