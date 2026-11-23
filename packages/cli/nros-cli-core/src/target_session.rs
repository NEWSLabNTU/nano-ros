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
///
/// Above every image's own termination grace, so an image ends by its own
/// hand, printing what it reports on the way out, before this escalates. A
/// threadx-linux image calls the app's SIGTERM handler and then has
/// `NROS_THREADX_LINUX_TERM_GRACE_MS` (2000 ms) before it reports its stack
/// peaks and ends; this was 2 s, the same number, so the SIGKILL could land
/// first and the report was lost. The test harness's kill-after
/// (`nros_tests::process::KILL_GRACE`) is held to the same rule, and
/// `check-process-deadline` checks both.
// Issue 2868.
pub const TEARDOWN_GRACE: Duration = Duration::from_secs(3);

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
    /// Every part, each on some console line, in any order: `--expect`
    /// given more than once. Two outputs of one image that race each other
    /// (a service reply and an action result) cannot be graded by one
    /// pattern, because whichever lands first would end the run.
    All(Vec<Expect>),
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

    /// Every pattern in `parts`: the one itself when there is one, `None`
    /// when there are none.
    #[must_use]
    pub fn all(mut parts: Vec<Expect>) -> Option<Self> {
        match parts.len() {
            0 => None,
            1 => parts.pop(),
            _ => Some(Expect::All(parts)),
        }
    }

    /// Whether `line` (without its line ending) satisfies it. For
    /// [`Expect::All`], whether it satisfies one of the parts: which parts a
    /// session has seen is the grader's state, not the pattern's.
    #[must_use]
    pub fn matches(&self, line: &str) -> bool {
        match self {
            Expect::Substring(s) => line.contains(s.as_str()),
            Expect::Regex(r) => r.is_match(line),
            Expect::All(parts) => parts.iter().any(|p| p.matches(line)),
        }
    }

    /// The parts a grader waits for, one each.
    fn into_parts(self) -> Vec<Expect> {
        match self {
            Expect::All(parts) => parts.into_iter().flat_map(Expect::into_parts).collect(),
            one => vec![one],
        }
    }

    /// What kind of pattern it is, as the verb's flags name it: `text`
    /// (`--expect` / `--forbid`) or `regex` (`--expect-regex` /
    /// `--forbid-regex`); `all` for [`Expect::All`].
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Expect::Substring(_) => "text",
            Expect::Regex(_) => "regex",
            Expect::All(_) => "all",
        }
    }

    /// The pattern as the user wrote it; the parts of [`Expect::All`]
    /// joined with `` ` and ` ``.
    #[must_use]
    pub fn pattern(&self) -> String {
        match self {
            Expect::Substring(s) => s.clone(),
            Expect::Regex(r) => r.as_str().to_string(),
            Expect::All(parts) => parts
                .iter()
                .map(Expect::pattern)
                .collect::<Vec<_>>()
                .join("` and `"),
        }
    }
}

/// Issue 2863 -- what a session grades its console for: the expectation
/// (`--expect` / `--expect-regex`, all of it, to succeed) and the forbidden
/// patterns (`--forbid` / `--forbid-regex`, any ONE of which fails the run,
/// before or after the expectation is met). Each forbidden pattern is a
/// single [`Expect::Substring`] or [`Expect::Regex`].
#[derive(Debug, Clone, Default)]
pub struct Grade {
    /// Every part must appear; `None` grades nothing for success.
    pub expect: Option<Expect>,
    /// The first line any of these matches ends the session as
    /// [`Outcome::Forbidden`].
    pub forbid: Vec<Expect>,
}

impl Grade {
    /// Grading for `expect` alone.
    #[must_use]
    pub fn expecting(expect: Option<Expect>) -> Self {
        Self {
            expect,
            forbid: Vec::new(),
        }
    }

    /// Whether anything is graded at all.
    #[must_use]
    pub fn grades(&self) -> bool {
        self.expect.is_some() || !self.forbid.is_empty()
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
    /// Issue 2863 -- a console line matched a forbidden pattern (sent once,
    /// for the first such line).
    Forbidden {
        /// The pattern as given.
        pattern: String,
        /// `text` or `regex` ([`Expect::kind`]).
        kind: &'static str,
        /// The line.
        line: String,
    },
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
    grade: Grade,
    tx: Sender<Event>,
) -> JoinHandle<()>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut from = from;
        let mut buf = [0u8; 8192];
        let mut line: Vec<u8> = Vec::new();
        let mut grading = Grading::new(grade.expect);
        let mut forbid = Forbid::new(grade.forbid);
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
            // Forbidden lines first: a chunk that completes the expectation
            // and holds a forbidden line fails, whichever came first in it.
            forbid.feed(chunk, &tx);
            grading.feed(chunk, &mut line, &tx);
        }
        forbid.finish(&tx);
        if let Grading::Finishing = grading {
            let _ = tx.send(Event::Matched(line_text(&line)));
        }
        let _ = tx.send(Event::ConsoleClosed);
    })
}

/// Issue 2863 -- [`spawn_console`]'s forbidden-line watch: its own line
/// buffer, so it keeps watching after the expectation is met. Sends
/// [`Event::Forbidden`] for the first line any pattern matches, then stops.
struct Forbid {
    patterns: Vec<Expect>,
    line: Vec<u8>,
    done: bool,
}

impl Forbid {
    fn new(patterns: Vec<Expect>) -> Self {
        let done = patterns.is_empty();
        Self {
            patterns,
            line: Vec::new(),
            done,
        }
    }

    fn check(&mut self, tx: &Sender<Event>) {
        let text = line_text(&self.line);
        if let Some(p) = self.patterns.iter().find(|p| p.matches(&text)) {
            let _ = tx.send(Event::Forbidden {
                pattern: p.pattern(),
                kind: p.kind(),
                line: text,
            });
            self.done = true;
        }
    }

    fn feed(&mut self, chunk: &[u8], tx: &Sender<Event>) {
        for &b in chunk {
            if self.done {
                return;
            }
            if b == b'\n' {
                self.check(tx);
                self.line.clear();
            } else {
                self.line.push(b);
            }
        }
    }

    /// The stream ended: its unterminated last line is a line too.
    fn finish(&mut self, tx: &Sender<Event>) {
        if !self.done && !self.line.is_empty() {
            self.check(tx);
        }
    }
}

/// Where [`spawn_console`]'s grading stands.
enum Grading {
    /// These parts have not matched a line yet (never empty).
    Looking(Vec<Expect>),
    /// The unterminated line in the buffer matched; waiting for its newline.
    Finishing,
    /// [`Event::Matched`] was sent, or there was nothing to grade.
    Done,
}

impl Grading {
    fn new(expect: Option<Expect>) -> Self {
        match expect.map(Expect::into_parts) {
            Some(parts) if !parts.is_empty() => Grading::Looking(parts),
            _ => Grading::Done,
        }
    }

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
                Grading::Looking(parts) => {
                    // Every part this line satisfies is seen; the line that
                    // sees the last one ends the run.
                    let text = line_text(line);
                    parts.retain(|p| !p.matches(&text));
                    parts.is_empty()
                }
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
            // Only the LAST part may finish on a partial line: an earlier
            // part is counted when its line ends, above.
            Grading::Looking(parts) if parts.len() == 1 && parts[0].matches(&text) => {
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
    /// Issue 2863 -- a console line matched a forbidden pattern
    /// (`--forbid` / `--forbid-regex`).
    Forbidden {
        /// The pattern as given.
        pattern: String,
        /// `text` or `regex`.
        kind: &'static str,
        /// The line.
        line: String,
    },
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
            // The target printed what it must not: a target failure, as an
            // unmet expectation is.
            Outcome::Forbidden { .. } => exit::TARGET_FAILED,
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
            Outcome::Forbidden {
                pattern,
                kind,
                line,
            } => {
                let flag = if *kind == "regex" {
                    "--forbid-regex"
                } else {
                    "--forbid"
                };
                format!("forbidden {kind} `{pattern}` ({flag}) seen after {t:.1} s: {line}")
            }
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
    /// Issue 2863 -- `--settle`: once the expectation is met, keep watching
    /// this long (a later forbidden line, the rest of a report) before the
    /// session ends as matched. `--timeout` bounds the time to the match only.
    pub settle: Option<Duration>,
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
            settle: None,
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
        // Issue 2863 -- the expectation is met and `--settle` keeps watching
        // until this instant; the line it was met on.
        let mut settling: Option<(String, Instant)> = None;
        let settle = self.settle.filter(|d| !d.is_zero());
        // The expectation is met on `line`: the outcome now, or `None` when a
        // settle window opens instead.
        let met = |line: String, settling: &mut Option<(String, Instant)>| match settle {
            Some(d) => {
                *settling = Some((line, Instant::now() + d));
                None
            }
            None => Some(Outcome::Matched(line)),
        };
        loop {
            if interrupted() {
                return Outcome::Interrupted;
            }
            if let Some((line, until)) = &settling {
                if Instant::now() >= *until {
                    return Outcome::Matched(line.clone());
                }
            } else {
                if let Some((line, at)) = &finishing
                    && (at.elapsed() >= LINE_FINISH || console_closed)
                {
                    let line = line.clone();
                    finishing = None;
                    if let Some(o) = met(line, &mut settling) {
                        return o;
                    }
                }
                // `--timeout` bounds the time to the match, never a settle
                // window that a match opened.
                if settling.is_none() && deadline.is_some_and(|d| Instant::now() >= d) {
                    match finishing.take() {
                        Some((line, _)) => {
                            if let Some(o) = met(line, &mut settling) {
                                return o;
                            }
                        }
                        None => return Outcome::TimedOut,
                    }
                }
            }
            match rx.recv_timeout(Duration::from_millis(50)) {
                Ok(Event::Forbidden {
                    pattern,
                    kind,
                    line,
                }) => {
                    return Outcome::Forbidden {
                        pattern,
                        kind,
                        line,
                    };
                }
                Ok(Event::Matched(line)) => {
                    if settling.is_none() {
                        finishing = None;
                        if let Some(o) = met(line, &mut settling) {
                            return o;
                        }
                    }
                }
                Ok(Event::Matching(line)) => {
                    if settling.is_none() {
                        let at = finishing.map_or_else(Instant::now, |(_, at)| at);
                        finishing = Some((line, at));
                    }
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
                    // The expectation was met: that verdict stands.
                    if let Some((line, _)) = settling.take().or_else(|| finishing.take()) {
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
                // One more look for a match, or a forbidden line, that raced
                // the close.
                let mut late: Option<String> = None;
                while let Ok(ev) = rx.try_recv() {
                    match ev {
                        Event::Forbidden {
                            pattern,
                            kind,
                            line,
                        } => {
                            return Outcome::Forbidden {
                                pattern,
                                kind,
                                line,
                            };
                        }
                        Event::Matched(line) => late = late.or(Some(line)),
                        Event::Matching(line) => {
                            let at = finishing.map_or_else(Instant::now, |(_, at)| at);
                            finishing = Some((line, at));
                        }
                        _ => {}
                    }
                }
                // Issue 2863 -- the target ended inside the settle window: a
                // clean exit keeps the match, a failing one is the verdict.
                if let Some((line, _)) = settling {
                    return if code == Some(0) {
                        Outcome::Matched(line)
                    } else {
                        Outcome::Exited(code)
                    };
                }
                if let Some(line) = late.or_else(|| finishing.map(|(l, _)| l)) {
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
        start_graded(script, Grade::expecting(expect))
    }

    fn start_graded(script: &str, grade: Grade) -> (Session, Receiver<Event>, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let mut group = ProcessGroup::new();
        let m = group.spawn("runner", sh(script)).unwrap();
        let out = m.child.stdout.take().unwrap();
        let err = m.child.stderr.take().unwrap();
        let (tx, rx) = channel();
        let log = File::create(dir.path().join("console.log")).unwrap();
        spawn_console(out, Box::new(io::sink()), Some(log), grade, tx.clone());
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
        // Issue 2665 -- this bounds the SETUP (a shell, its `setsid` and the
        // chain's first process getting scheduled), not the behaviour under
        // test, so it is long: a chain that never starts still fails. 5 s was
        // missed at load average ~390.
        runner_with_chain_seen_after(chain, Duration::ZERO, Duration::from_secs(60))
    }

    /// [`runner_with_chain`], looking for the chain only after `look_after`
    /// (what a loaded host does to the helper by accident), within `setup`.
    #[cfg(target_os = "linux")]
    fn runner_with_chain_seen_after(
        chain: &str,
        look_after: Duration,
        setup: Duration,
    ) -> (ProcessGroup, i32) {
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
        let deadline = Instant::now() + setup;
        let pgid = loop {
            if let Some(&g) = chains.0.lock().unwrap().first() {
                break g;
            }
            assert!(Instant::now() < deadline, "no chain line");
            thread::sleep(Duration::from_millis(10));
        };
        thread::sleep(look_after);
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
        //
        // Issue 2665 -- the chain's half second starts only once the test
        // says go. It used to start at once, so under load the chain could
        // finish before `runner_with_chain` first looked for it, and an empty
        // group then read as "the chain never started" for as long as the
        // deadline allowed. Now the chain is alive and waiting whenever the
        // helper looks, and its half second overlaps the teardown, as meant.
        let dir = tempfile::tempdir().unwrap();
        let mark = dir.path().join("ended");
        let go = dir.path().join("go");
        let (mut group, pgid) = runner_with_chain(&format!(
            "sh -c 'while [ ! -e {go} ]; do sleep 0.01; done; sleep 0.5; touch {mark}'",
            go = go.display(),
            mark = mark.display()
        ));
        std::fs::write(&go, b"").unwrap();
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

    /// Issue 2665, the race made deterministic: a chain that ends on its own
    /// half a second after it starts is gone by the time a helper that looks
    /// a second late arrives, so the helper reads "the chain never started".
    /// This is what a loaded host did to the test below it by accident, and
    /// why that test's chain now waits for the test's go.
    #[cfg(target_os = "linux")]
    #[test]
    #[should_panic(expected = "the chain never started")]
    fn a_short_chain_is_gone_before_a_late_look() {
        let _ = runner_with_chain_seen_after(
            "sleep 0.5",
            Duration::from_secs(1),
            Duration::from_secs(3),
        );
    }

    /// Issue 2665 -- the gated chain the teardown test now uses is seen
    /// however late the helper looks: it lives until the test says go.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_gated_chain_is_seen_by_a_late_look() {
        let dir = tempfile::tempdir().unwrap();
        let go = dir.path().join("go");
        let (mut group, pgid) = runner_with_chain_seen_after(
            &format!(
                "sh -c 'while [ ! -e {go} ]; do sleep 0.01; done'",
                go = go.display()
            ),
            Duration::from_secs(1),
            Duration::from_secs(3),
        );
        assert!(!group_members(pgid).is_empty(), "the gated chain is alive");
        std::fs::write(&go, b"").unwrap();
        group.teardown(TEARDOWN_GRACE);
        assert!(
            group_members(pgid).is_empty(),
            "the chain outlived the teardown"
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
        let mut g = Grading::new(Some(Expect::substring("Pub")));
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

    /// `--expect` twice: the run ends on the line that completes the set,
    /// whichever of the two lands first.
    #[test]
    fn every_part_must_match_in_any_order() {
        for (first, second) in [("sum: 1", "result last=55"), ("result last=55", "sum: 1")] {
            let (tx, rx) = channel();
            let mut g = Grading::new(Expect::all(vec![
                Expect::substring("sum:"),
                Expect::substring("result last="),
            ]));
            let mut line = Vec::new();
            g.feed(format!("boot\n{first}\nnoise\n").as_bytes(), &mut line, &tx);
            assert!(matches!(&g, Grading::Looking(p) if p.len() == 1));
            g.feed(format!("{second}\nafter\n").as_bytes(), &mut line, &tx);
            drop(tx);
            let got: Vec<Event> = rx.iter().collect();
            assert_eq!(got, [Event::Matched(second.into())]);
            assert!(matches!(g, Grading::Done));
        }
        // One line may satisfy both.
        let (tx, rx) = channel();
        let mut g = Grading::new(Expect::all(vec![
            Expect::substring("a"),
            Expect::substring("b"),
        ]));
        g.feed(b"ab\n", &mut Vec::new(), &tx);
        drop(tx);
        assert_eq!(rx.iter().collect::<Vec<_>>(), [Event::Matched("ab".into())]);
        assert_eq!(
            Expect::all(vec![Expect::substring("x"), Expect::substring("y")])
                .unwrap()
                .pattern(),
            "x` and `y"
        );
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

    // -- Issue 2863: --forbid and --settle --

    fn graded(expect: Option<&str>, forbid: &[&str]) -> Grade {
        Grade {
            expect: expect.map(Expect::substring),
            forbid: forbid.iter().map(|f| Expect::substring(f)).collect(),
        }
    }

    /// (a) A forbidden line before the expectation ends the session at once:
    /// no waiting for the match or the timeout.
    #[test]
    fn a_forbidden_line_before_the_match_fails_at_once() {
        let _g = FLAG
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let (mut s, rx, _dir) = start_graded(
            "echo 'ZEPHYR FATAL ERROR'; echo 'tests passed'; sleep 30",
            graded(Some("tests passed"), &["FATAL"]),
        );
        let t = Instant::now();
        let o = s.watch(&rx, t, |_, _| Ok(()));
        s.group.teardown(TEARDOWN_GRACE);
        assert!(
            matches!(&o, Outcome::Forbidden { line, .. } if line == "ZEPHYR FATAL ERROR"),
            "{o:?}"
        );
        assert_eq!(o.exit_code(true), exit::TARGET_FAILED);
        assert!(t.elapsed() < Duration::from_secs(5), "{:?}", t.elapsed());
    }

    /// A forbidden line after the match, inside the settle window, fails the
    /// run; without `--settle` the run had already ended on the match.
    #[test]
    fn a_forbidden_line_inside_the_settle_window_fails() {
        let _g = FLAG
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let (mut s, rx, _dir) = start_graded(
            "echo 'tests passed'; sleep 0.3; echo 'ZEPHYR FATAL ERROR'; sleep 30",
            graded(Some("tests passed"), &["FATAL"]),
        );
        s.settle = Some(Duration::from_secs(3));
        let o = s.watch(&rx, Instant::now(), |_, _| Ok(()));
        s.group.teardown(TEARDOWN_GRACE);
        assert!(matches!(o, Outcome::Forbidden { .. }), "{o:?}");
        assert_eq!(o.exit_code(true), exit::TARGET_FAILED);
        assert!(
            o.describe(None, Duration::from_secs(1))
                .contains("forbidden text `FATAL` (--forbid)"),
            "{}",
            o.describe(None, Duration::from_secs(1))
        );
    }

    /// A clean settle window ends as matched, after the window and not at the
    /// match; `--timeout` does not cut it short.
    #[test]
    fn a_clean_settle_window_ends_matched_after_the_window() {
        let _g = FLAG
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let (mut s, rx, _dir) = start_graded(
            "echo 'tests passed'; sleep 30",
            graded(Some("tests passed"), &["FATAL"]),
        );
        s.timeout = Some(Duration::from_millis(200));
        s.settle = Some(Duration::from_millis(800));
        let t = Instant::now();
        let o = s.watch(&rx, t, |_, _| Ok(()));
        s.group.teardown(TEARDOWN_GRACE);
        assert_eq!(o, Outcome::Matched("tests passed".into()));
        assert_eq!(o.exit_code(true), exit::SUCCESS);
        assert!(
            t.elapsed() >= Duration::from_millis(800),
            "{:?}",
            t.elapsed()
        );
    }

    /// The target ending inside the window: a failing exit is the verdict, a
    /// clean one keeps the match.
    #[test]
    fn a_target_that_fails_inside_the_settle_window_fails_the_run() {
        let _g = FLAG
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for (code, want) in [
            (3, Outcome::Exited(Some(3))),
            (0, Outcome::Matched("tests passed".into())),
        ] {
            let (mut s, rx, _dir) = start_graded(
                &format!("echo 'tests passed'; sleep 0.2; exit {code}"),
                graded(Some("tests passed"), &[]),
            );
            s.settle = Some(Duration::from_secs(10));
            let o = s.watch(&rx, Instant::now(), |_, _| Ok(()));
            s.group.teardown(TEARDOWN_GRACE);
            assert_eq!(o, want, "exit {code}");
        }
    }

    /// `--forbid` alone: no expectation, and a forbidden line still fails.
    #[test]
    fn a_forbidden_line_fails_a_run_with_no_expectation() {
        let _g = FLAG
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let (mut s, rx, _dir) = start_graded(
            "echo booting; echo 'panic!'; sleep 30",
            graded(None, &["panic"]),
        );
        let o = s.watch(&rx, Instant::now(), |_, _| Ok(()));
        s.group.teardown(TEARDOWN_GRACE);
        assert!(matches!(o, Outcome::Forbidden { .. }), "{o:?}");
        assert_eq!(o.exit_code(false), exit::TARGET_FAILED);
    }
}
