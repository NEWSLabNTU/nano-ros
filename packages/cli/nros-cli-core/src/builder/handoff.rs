//! Stage 5 — hand the build to the native tool (phase-383 W2.e, RFC-0065 D1,
//! amended 2026-11-23 by phase-588).
//!
//! ## What this file promises
//!
//! `nros build` is admissible at all only because of what happens here.
//! RFC-0024 §2.4 marks "nros never a build verb" LOCKED, and §9 rejects it as
//! *"re-creates colcon's wrapping anti-pattern; hides cargo/cmake
//! diagnostics."* RFC-0065 amends that in SCOPE, not in principle, on one
//! claim: **the native tool's diagnostics reach the user byte-identical, and
//! its exit status is the build's.**
//!
//! How that claim is kept has changed twice:
//!
//! 1. phase-383: stage 5 was `execvp`, so nothing could capture anything.
//! 2. phase-485 W3: the tool became a CHILD with inherited stdio, because the
//!    project lock is recorded after the build (RFC-0104 D4) and a process
//!    that has become the compiler can record nothing.
//! 3. phase-588 (RFC-0065 D1 amendment, issue 2836): the child runs on a
//!    pseudo-terminal `nros build` holds, and every byte it writes is copied
//!    to our stdout AND to `<image build dir>/<tool>.log`. The log is read
//!    only after a failed build, to explain what the build already knows
//!    (a memory-region overflow at link: [`super::link_explain`]).
//!
//! ## Why a pty, and never a plain pipe
//!
//! A pipe would change what the tool does, not just where its bytes go: the
//! tool would see no TTY, so cargo drops its colour and its progress bar and
//! C compilers switch to full buffering. A pty keeps every one of those. The
//! tool's TTY has the caller's size, and a resize of the caller's terminal is
//! passed on (`SIGWINCH` → `TIOCSWINSZ`).
//!
//! **The copy is a copy.** [`proxy`] writes each chunk it reads, unchanged,
//! to both sinks: no line is filtered, rewritten, recoloured or summarised.
//! That, not the absence of a second process, is what the RFC-0024
//! reconciliation rests on. A progress bar or summary line of our own would
//! void it, and `stage_five_copies_and_never_filters` is the guard.
//!
//! Where `nros build`'s own stdout is not a terminal (CI, `| tee`), the tool
//! gets none either, exactly as it would without us: its stdout and stderr
//! are pipes, still teed to the log, each to its own stream.
//!
//! ## Signals and the exit status
//!
//! The child gets its own session (and, on a pty, the pty as its controlling
//! terminal), so the terminal's Ctrl-C reaches `nros build` alone. `SIGINT`
//! and `SIGTERM` are forwarded to the child's whole process group, which is
//! what ninja, make and cargo put their compilers in: a Ctrl-C still stops
//! the compiler and leaves no orphan. Outside a wait the signals keep their
//! default meaning. The exit status returned is the child's own; the caller
//! reports a code as that code and a signal as 128 + the signal.
//!
//! ## Non-Unix
//!
//! nano-ros builds host-side on Unix only. Elsewhere the child inherits stdio
//! and no log is written, which keeps the diagnostics promise and drops only
//! the explanation.

use std::{ffi::OsString, path::PathBuf};

/// A fully-resolved native build command, ready to replace this process.
///
/// Deliberately inert: constructing one performs no I/O, so a caller can build
/// it, print it (`--dry-run`), assert on it in a test, and only then hand over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Handoff {
    /// Program to exec — `cargo`, `cmake`, `west`. Resolved through
    /// `PATH` by `execvp`, deliberately: the user's activated environment is
    /// what should decide which `west` runs, not a path we baked at build time.
    pub program: OsString,
    /// Arguments, NOT including argv[0].
    pub args: Vec<OsString>,
    /// Working directory to change into first. `None` keeps the caller's.
    pub cwd: Option<PathBuf>,
    /// Environment additions applied before the exec. Never a full env
    /// replacement: a build needs the user's `PATH`, `HOME`, `SSH_AUTH_SOCK`
    /// and the whole activated toolchain environment.
    pub env: Vec<(OsString, OsString)>,
    /// The build's shared jobserver ([`super::jobserver`]): set or removed
    /// after [`Self::env`]. Kept apart from it so [`Self::display`] stays what
    /// a user would retype — the pool's per-run path means nothing outside
    /// this process.
    pub jobserver: super::jobserver::ChildEnv,
    /// Where [`wait`] keeps the tool's output (`<image build dir>/<tool>.log`),
    /// or `None` to run it with inherited stdio and keep nothing. Set by
    /// stage 5 from the plan (`cmd::build::with_log`), never by a planner, and
    /// kept out of [`Self::display`]: it is not part of the command.
    pub log: Option<PathBuf>,
}

impl Handoff {
    /// A handoff to `program` with `args`.
    pub fn new<P, A, I>(program: P, args: I) -> Self
    where
        P: Into<OsString>,
        A: Into<OsString>,
        I: IntoIterator<Item = A>,
    {
        Self {
            program: program.into(),
            args: args.into_iter().map(Into::into).collect(),
            cwd: None,
            env: Vec::new(),
            jobserver: Vec::new(),
            log: None,
        }
    }

    /// Run the command from `dir`.
    #[must_use]
    pub fn in_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.cwd = Some(dir.into());
        self
    }

    /// Add one environment variable.
    #[must_use]
    pub fn with_env(mut self, key: impl Into<OsString>, value: impl Into<OsString>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }

    /// Share `env` (from [`super::jobserver::Jobserver::child_env`]) with the
    /// command.
    #[must_use]
    pub fn with_jobserver(mut self, env: &super::jobserver::ChildEnv) -> Self {
        self.jobserver = env.clone();
        self
    }

    /// The same handoff with `--offline` dropped, or `None` when it
    /// carried none.
    ///
    /// For escalating an offline step that failed for want of a warm
    /// registry cache (issue 0873). `None` is the signal that offline
    /// was not why it failed, so there is nothing to retry — never a
    /// silent no-op retry of the identical command.
    #[must_use]
    pub fn without_offline(&self) -> Option<Self> {
        if !self.args.iter().any(|a| a == "--offline") {
            return None;
        }
        let mut next = self.clone();
        next.args.retain(|a| a != "--offline");
        Some(next)
    }

    /// Render the command the way a user could retype it.
    ///
    /// For `--dry-run` and for error messages. Each word is shell-quoted when
    /// it needs to be, so the line pastes into a POSIX shell as the same argv
    /// (issue 2692: a `--config` value with spaces fell apart for a user and
    /// for every script that splits this line). A plain word prints as is.
    #[must_use]
    pub fn display(&self) -> String {
        let mut s = String::new();
        for (k, v) in &self.env {
            s.push_str(&format!(
                "{}={} ",
                k.to_string_lossy(),
                shell_word(&v.to_string_lossy())
            ));
        }
        s.push_str(&shell_word(&self.program.to_string_lossy()));
        for a in &self.args {
            s.push(' ');
            s.push_str(&shell_word(&a.to_string_lossy()));
        }
        s
    }
}

impl Handoff {
    /// The same command as a `std::process::Command`, for a step that must
    /// RETURN.
    ///
    /// Stage 5 execs and never comes back, which is the guarantee RFC-0065 D1
    /// rests on. A driver that needs a configure FIRST (cmake: `cmake --build`
    /// on an unconfigured tree fails, and configure+build is two invocations at
    /// our 3.22 floor) needs to run one command and survive it. Same struct, so
    /// the configure a `--dry-run` prints is the configure that runs.
    #[must_use]
    pub fn command(&self) -> std::process::Command {
        let mut cmd = std::process::Command::new(&self.program);
        cmd.args(&self.args);
        if let Some(dir) = &self.cwd {
            cmd.current_dir(dir);
        }
        for (k, v) in &self.env {
            cmd.env(k, v);
        }
        apply_jobserver(&mut cmd, &self.jobserver);
        cmd
    }
}

/// The `Command` both handovers run, with what must hold BEFORE either one
/// checked once.
///
/// `exec` and [`wait`] differ only in how they reach the operating system;
/// everything they validate first is the same, so it is validated in one place.
/// A second copy is how the two would drift into reporting a missing build
/// directory differently.
fn prepared(handoff: &Handoff) -> Result<std::process::Command, String> {
    let mut cmd = std::process::Command::new(&handoff.program);
    cmd.args(&handoff.args);
    if let Some(dir) = &handoff.cwd {
        if !dir.is_dir() {
            return Err(format!(
                "build directory {} does not exist — stage 4 should have created it",
                dir.display()
            ));
        }
        cmd.current_dir(dir);
    }
    for (k, v) in &handoff.env {
        cmd.env(k, v);
    }
    apply_jobserver(&mut cmd, &handoff.jobserver);
    Ok(cmd)
}

/// Set or remove each jobserver variable on `cmd`. One spelling for both
/// [`Handoff::command`] and the handovers, so no child can miss the pool.
fn apply_jobserver(cmd: &mut std::process::Command, env: &super::jobserver::ChildEnv) {
    for (k, v) in env {
        match v {
            Some(v) => cmd.env(k, v),
            None => cmd.env_remove(k),
        };
    }
}

/// Run the native build tool as a child with inherited stdio and wait for it.
///
/// For a step whose output nobody reads back (a [`Handoff`] with no
/// [`Handoff::log`]): nothing is captured, and the status is the tool's.
pub fn run_inherited(handoff: &Handoff) -> Result<std::process::ExitStatus, String> {
    let mut cmd = prepared(handoff)?;
    cmd.status()
        .map_err(|e| format!("could not run `{}`: {e}", handoff.program.to_string_lossy()))
}

/// Run the native build tool on a pty `nros build` holds (or on pipes, when
/// our stdout is no terminal), copying every byte it writes to our stdout and
/// to `log`, and wait for it. Returns the tool's own exit status and the log
/// written, which holds the same bytes the terminal got.
///
/// `log` is truncated first: it is the LAST build's output, never a history.
/// A log that cannot be written does not fail the build; the bytes still
/// reach the terminal and the failure is reported once at the end.
#[cfg(unix)]
pub fn wait(
    handoff: &Handoff,
    log: &std::path::Path,
) -> Result<(std::process::ExitStatus, PathBuf), String> {
    let mode = if proxy::isatty(libc::STDOUT_FILENO) {
        proxy::Mode::Pty {
            size: proxy::winsize_of(libc::STDOUT_FILENO),
            stderr_too: proxy::isatty(libc::STDERR_FILENO),
        }
    } else {
        proxy::Mode::Pipes
    };
    let st = proxy::run(
        handoff,
        log,
        mode,
        &mut proxy::StdoutSink,
        &mut proxy::StderrSink,
    )?;
    Ok((st, log.to_path_buf()))
}

/// Non-Unix: inherited stdio, no log (see the module docs).
#[cfg(not(unix))]
pub fn wait(
    handoff: &Handoff,
    log: &std::path::Path,
) -> Result<(std::process::ExitStatus, PathBuf), String> {
    Ok((run_inherited(handoff)?, log.to_path_buf()))
}

/// The pty proxy behind [`wait`].
#[cfg(unix)]
mod proxy {
    use std::{
        fs::File,
        io::Write,
        os::{
            fd::{AsRawFd, FromRawFd, OwnedFd, RawFd},
            unix::process::CommandExt,
        },
        path::Path,
        process::{ExitStatus, Stdio},
        sync::{
            Once,
            atomic::{AtomicUsize, Ordering},
        },
    };

    use super::Handoff;

    /// How the child's output reaches us.
    #[derive(Debug, Clone, Copy)]
    pub(super) enum Mode {
        /// Our stdout is a terminal: the child's stdout is a pty of `size`,
        /// and so is its stderr when ours is a terminal too (else a pipe to
        /// our stderr, so `nros build 2>err` still splits the streams).
        Pty {
            size: libc::winsize,
            stderr_too: bool,
        },
        /// Our stdout is not a terminal: two pipes, as the child would get
        /// with no `nros build` in between... plus the log.
        Pipes,
    }

    /// Where a stream's bytes go besides the log. A trait so a test can
    /// collect what "the terminal" got.
    pub(super) trait Sink {
        fn put(&mut self, bytes: &[u8]);
    }

    pub(super) struct StdoutSink;
    impl Sink for StdoutSink {
        fn put(&mut self, bytes: &[u8]) {
            // A closed stdout (`nros build | head`) must not stop the build
            // or the log; the bytes are dropped there only.
            let mut o = std::io::stdout().lock();
            let _ = o.write_all(bytes).and_then(|()| o.flush());
        }
    }

    pub(super) struct StderrSink;
    impl Sink for StderrSink {
        fn put(&mut self, bytes: &[u8]) {
            let mut e = std::io::stderr().lock();
            let _ = e.write_all(bytes).and_then(|()| e.flush());
        }
    }

    impl Sink for Vec<u8> {
        fn put(&mut self, bytes: &[u8]) {
            self.extend_from_slice(bytes);
        }
    }

    pub(super) fn isatty(fd: RawFd) -> bool {
        // SAFETY: isatty reads no memory of ours.
        unsafe { libc::isatty(fd) == 1 }
    }

    /// `fd`'s terminal size, or 24x80 when it has none.
    pub(super) fn winsize_of(fd: RawFd) -> libc::winsize {
        let mut ws = libc::winsize {
            ws_row: 0,
            ws_col: 0,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        // SAFETY: TIOCGWINSZ writes one `winsize` into `ws`.
        let ok = unsafe { libc::ioctl(fd, libc::TIOCGWINSZ, &raw mut ws) } == 0;
        if !ok || ws.ws_row == 0 || ws.ws_col == 0 {
            ws.ws_row = 24;
            ws.ws_col = 80;
        }
        ws
    }

    // ---- signals -------------------------------------------------------
    //
    // One handler for the process, installed on the first wait. It only
    // counts (atomics are async-signal-safe); each running proxy forwards
    // what it has not yet seen, so several concurrent waits (parallel package
    // plans) each pass a Ctrl-C to their own child. With no wait running, a
    // SIGINT or SIGTERM gets its default meaning back and is re-raised, so
    // `nros build` between two plans dies of Ctrl-C exactly as before.

    /// Proxies running now.
    pub(super) static ACTIVE: AtomicUsize = AtomicUsize::new(0);
    pub(super) static SIGINTS: AtomicUsize = AtomicUsize::new(0);
    pub(super) static SIGTERMS: AtomicUsize = AtomicUsize::new(0);
    pub(super) static SIGWINCHES: AtomicUsize = AtomicUsize::new(0);

    extern "C" fn on_signal(sig: libc::c_int) {
        if sig == libc::SIGWINCH {
            SIGWINCHES.fetch_add(1, Ordering::SeqCst);
            return;
        }
        if ACTIVE.load(Ordering::SeqCst) == 0 {
            // SAFETY: signal() and raise() are async-signal-safe.
            unsafe {
                libc::signal(sig, libc::SIG_DFL);
                libc::raise(sig);
            }
            return;
        }
        if sig == libc::SIGINT {
            SIGINTS.fetch_add(1, Ordering::SeqCst);
        } else {
            SIGTERMS.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn install_handlers() {
        static ONCE: Once = Once::new();
        ONCE.call_once(|| {
            for sig in [libc::SIGINT, libc::SIGTERM, libc::SIGWINCH] {
                // SAFETY: sigaction with a zeroed struct plus our handler;
                // `old` receives the previous disposition.
                unsafe {
                    let mut old: libc::sigaction = std::mem::zeroed();
                    if libc::sigaction(sig, std::ptr::null(), &raw mut old) != 0 {
                        continue;
                    }
                    // A signal the caller set to IGNORE (`nohup`, a
                    // background job) stays ignored: forwarding it would
                    // deliver what the caller asked never to deliver.
                    if old.sa_sigaction == libc::SIG_IGN {
                        continue;
                    }
                    let mut act: libc::sigaction = std::mem::zeroed();
                    act.sa_sigaction = on_signal as extern "C" fn(libc::c_int) as usize;
                    libc::sigemptyset(&raw mut act.sa_mask);
                    act.sa_flags = libc::SA_RESTART;
                    libc::sigaction(sig, &raw const act, std::ptr::null_mut());
                }
            }
        });
    }

    /// Counts [`ACTIVE`] for as long as a proxy runs, including when it
    /// returns early.
    struct Active;
    impl Active {
        fn enter() -> Self {
            ACTIVE.fetch_add(1, Ordering::SeqCst);
            Active
        }
    }
    impl Drop for Active {
        fn drop(&mut self) {
            ACTIVE.fetch_sub(1, Ordering::SeqCst);
        }
    }

    fn os_err(what: &str) -> String {
        format!("{what}: {}", std::io::Error::last_os_error())
    }

    fn cloexec(fd: RawFd) {
        // SAFETY: plain fcntl on a descriptor we own.
        unsafe {
            libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC);
        }
    }

    /// A new pty of `size`: (master, slave).
    fn open_pty(size: libc::winsize) -> Result<(OwnedFd, OwnedFd), String> {
        let (mut m, mut s) = (-1, -1);
        // SAFETY: openpty writes two descriptors; name and termios are NULL.
        let rc = unsafe {
            libc::openpty(
                &raw mut m,
                &raw mut s,
                std::ptr::null_mut(),
                std::ptr::null(),
                &raw const size,
            )
        };
        if rc != 0 {
            return Err(os_err("could not open a pseudo-terminal for the build"));
        }
        cloexec(m);
        cloexec(s);
        // SAFETY: both were just returned by openpty and are owned by nobody
        // else.
        Ok(unsafe { (OwnedFd::from_raw_fd(m), OwnedFd::from_raw_fd(s)) })
    }

    /// Which sink a source's bytes go to.
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum To {
        Out,
        Err,
    }

    /// Run `handoff` in `mode`, teeing to `out`/`err` and `log`; see
    /// [`super::wait`].
    pub(super) fn run(
        handoff: &Handoff,
        log_path: &Path,
        mode: Mode,
        out: &mut dyn Sink,
        err: &mut dyn Sink,
    ) -> Result<ExitStatus, String> {
        let mut cmd = super::prepared(handoff)?;
        if let Some(dir) = log_path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let mut log = File::create(log_path).map_err(|e| e.to_string());
        install_handlers();

        let mut sources: Vec<(OwnedFd, To)> = Vec::new();
        let mut master: Option<RawFd> = None;
        let mut pty_slave: Option<OwnedFd> = None;
        match mode {
            Mode::Pty { size, stderr_too } => {
                let (m, s) = open_pty(size)?;
                let dup = |s: &OwnedFd| {
                    s.try_clone()
                        .map(Stdio::from)
                        .map_err(|e| format!("could not share the build's pty: {e}"))
                };
                cmd.stdout(dup(&s)?);
                if stderr_too {
                    cmd.stderr(dup(&s)?);
                } else {
                    cmd.stderr(Stdio::piped());
                }
                // SAFETY: the closure runs in the child between fork and
                // exec, after stdio is in place, and calls only
                // async-signal-safe functions.
                unsafe {
                    cmd.pre_exec(|| {
                        if libc::setsid() < 0 {
                            return Err(std::io::Error::last_os_error());
                        }
                        // fd 1 is the pty's slave by now: make it this
                        // session's controlling terminal, so a resize
                        // delivers SIGWINCH to the tool.
                        if libc::ioctl(libc::STDOUT_FILENO, libc::TIOCSCTTY, 0) < 0 {
                            return Err(std::io::Error::last_os_error());
                        }
                        Ok(())
                    });
                }
                master = Some(m.as_raw_fd());
                sources.push((m, To::Out));
                pty_slave = Some(s);
            }
            Mode::Pipes => {
                cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
                // SAFETY: as above; setsid only.
                unsafe {
                    cmd.pre_exec(|| {
                        if libc::setsid() < 0 {
                            return Err(std::io::Error::last_os_error());
                        }
                        Ok(())
                    });
                }
            }
        }

        // Count before the spawn, so a Ctrl-C between it and the loop is
        // forwarded rather than killing `nros build` over a live child.
        let _active = Active::enter();
        let mut seen = (
            SIGINTS.load(Ordering::SeqCst),
            SIGTERMS.load(Ordering::SeqCst),
            SIGWINCHES.load(Ordering::SeqCst),
        );
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("could not run `{}`: {e}", handoff.program.to_string_lossy()))?;
        // The child holds its own copies; ours would keep the pty open past
        // its exit, and a read would never see the end.
        drop(cmd);
        drop(pty_slave);
        if let Some(p) = child.stdout.take() {
            sources.push((OwnedFd::from(p), To::Out));
        }
        if let Some(p) = child.stderr.take() {
            sources.push((OwnedFd::from(p), To::Err));
        }
        let pgid = child.id() as libc::pid_t;

        let mut log_failed: Option<String> = log.as_ref().err().cloned();
        let mut buf = vec![0u8; 64 * 1024];
        let mut status: Option<ExitStatus> = None;
        loop {
            // Forward what arrived since the last turn.
            let now = (
                SIGINTS.load(Ordering::SeqCst),
                SIGTERMS.load(Ordering::SeqCst),
                SIGWINCHES.load(Ordering::SeqCst),
            );
            if status.is_none() {
                // SAFETY: kill on the child's own process group.
                if now.0 != seen.0 {
                    unsafe { libc::kill(-pgid, libc::SIGINT) };
                }
                if now.1 != seen.1 {
                    unsafe { libc::kill(-pgid, libc::SIGTERM) };
                }
            }
            if now.2 != seen.2 {
                if let Some(m) = master {
                    let ws = winsize_of(libc::STDOUT_FILENO);
                    // SAFETY: TIOCSWINSZ reads one `winsize`; the kernel
                    // signals the pty's foreground group.
                    unsafe { libc::ioctl(m, libc::TIOCSWINSZ, &raw const ws) };
                }
            }
            seen = now;

            if status.is_none() {
                status = child
                    .try_wait()
                    .map_err(|e| format!("could not wait for the build: {e}"))?;
            }
            if sources.is_empty() && status.is_some() {
                break;
            }
            let mut fds: Vec<libc::pollfd> = sources
                .iter()
                .map(|(fd, _)| libc::pollfd {
                    fd: fd.as_raw_fd(),
                    events: libc::POLLIN,
                    revents: 0,
                })
                .collect();
            // Once the tool has exited, a source that stays silent is held
            // open by something that outlived it (a daemon such as a compiler
            // cache server): stop reading rather than wait for it forever.
            let timeout = if status.is_some() { 50 } else { 100 };
            // SAFETY: `fds` is a live slice of `fds.len()` pollfds.
            let n = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, timeout) };
            if n < 0 {
                if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(os_err("could not read the build's output"));
            }
            if n == 0 {
                if status.is_some() {
                    break;
                }
                continue;
            }
            let mut closed = Vec::new();
            for (i, p) in fds.iter().enumerate() {
                if p.revents == 0 {
                    continue;
                }
                // SAFETY: reads at most `buf.len()` bytes into `buf`.
                let r = unsafe { libc::read(p.fd, buf.as_mut_ptr().cast(), buf.len()) };
                if r > 0 {
                    let bytes = &buf[..r as usize];
                    match sources[i].1 {
                        To::Out => out.put(bytes),
                        To::Err => err.put(bytes),
                    }
                    if let Ok(f) = &mut log {
                        if let Err(e) = f.write_all(bytes) {
                            log_failed.get_or_insert(e.to_string());
                        }
                    }
                    continue;
                }
                let e = std::io::Error::last_os_error();
                if r < 0
                    && matches!(
                        e.kind(),
                        std::io::ErrorKind::Interrupted | std::io::ErrorKind::WouldBlock
                    )
                {
                    continue;
                }
                // 0 is a pipe's end; EIO is a pty's, once every slave closed.
                closed.push(i);
            }
            for i in closed.into_iter().rev() {
                sources.remove(i);
            }
        }
        let status = match status {
            Some(s) => s,
            None => child
                .wait()
                .map_err(|e| format!("could not wait for the build: {e}"))?,
        };
        if let Some(e) = log_failed {
            err.put(
                format!(
                    "nros build: the build's output was not kept in {}: {e}\n",
                    log_path.display()
                )
                .as_bytes(),
            );
        }
        Ok(status)
    }
}

/// `w` as one POSIX shell word: unchanged when every character is one a
/// shell leaves alone, else single-quoted (a `'` inside becomes `'\''`).
fn shell_word(w: &str) -> String {
    let plain = !w.is_empty()
        && w.chars().all(|c| {
            c.is_ascii_alphanumeric()
                || matches!(c, '_' | '-' | '.' | '/' | ':' | ',' | '=' | '+' | '@' | '%')
        });
    if plain {
        w.to_string()
    } else {
        format!("'{}'", w.replace('\'', "'\\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_handoff_is_inert_until_exec() {
        // Constructing one must do no I/O: `--dry-run` prints it, tests assert
        // on it, and only then does anyone hand over.
        let h = Handoff::new("cargo", ["build", "-p", "linux_entry"])
            .in_dir("/nonexistent/build/dir")
            .with_env("CARGO_TERM_COLOR", "always");
        assert_eq!(h.program, OsString::from("cargo"));
        assert_eq!(h.args.len(), 3);
        assert_eq!(
            h.cwd.as_deref(),
            Some(std::path::Path::new("/nonexistent/build/dir"))
        );
    }

    #[test]
    fn display_is_retypable_by_a_human() {
        let h = Handoff::new("west", ["build", "-b", "native_sim/native/64"])
            .with_env("ZEPHYR_BASE", "/opt/zephyr");
        assert_eq!(
            h.display(),
            "ZEPHYR_BASE=/opt/zephyr west build -b native_sim/native/64"
        );
    }

    #[test]
    fn a_missing_build_dir_fails_before_handing_over() {
        // The error must name stage 4, because that is who was supposed to
        // create the directory.
        let h = Handoff::new("cargo", ["build"]).in_dir("/definitely/not/here");
        let e = run_inherited(&h).expect_err("must refuse");
        assert!(e.contains("/definitely/not/here"), "{e}");
        assert!(
            e.contains("stage 4"),
            "points at the responsible stage: {e}"
        );
        let log = tempfile::tempdir().unwrap();
        let e = wait(&h, &log.path().join("cargo.log")).expect_err("must refuse");
        assert!(e.contains("stage 4"), "{e}");
    }

    #[test]
    fn an_unfindable_program_reports_the_program_name() {
        let h = Handoff::new("nros-no-such-build-tool-exists", ["build"]);
        let e = run_inherited(&h).expect_err("must fail");
        assert!(e.contains("nros-no-such-build-tool-exists"), "{e}");
        let log = tempfile::tempdir().unwrap();
        let e = wait(&h, &log.path().join("x.log")).expect_err("must fail");
        assert!(e.contains("nros-no-such-build-tool-exists"), "{e}");
    }

    /// The guard for the RFC-0024 §2.4 reconciliation, half one: nothing in
    /// this file swallows the tool's output or replaces it with its own.
    ///
    /// A source scan, deliberately: the property is "this file never drops a
    /// stream", and no runtime test can prove a negative about code paths it
    /// does not take. Half two is [`stage_five_copies_and_never_filters`].
    #[test]
    fn stage_five_never_swallows_output() {
        let src = include_str!("handoff.rs");
        // Only CODE is scanned. Doc comments legitimately name the forbidden
        // constructs.
        let code: String = src
            .lines()
            .filter(|l| {
                let t = l.trim_start();
                !t.starts_with("//") && !t.starts_with("*") && !t.starts_with("#!")
            })
            .take_while(|l| !l.contains("fn stage_five_never_swallows_output"))
            .collect::<Vec<_>>()
            .join("\n");
        for forbidden in ["Stdio::null", ".output()", "Stdio::inherit"] {
            assert!(
                !code.contains(forbidden),
                "stage 5 must hand every byte of the native tool's output on \
                 — found `{forbidden}`. See the module docs (RFC-0065 D1)."
            );
        }
    }

    #[cfg(unix)]
    fn proxied(
        script: &str,
        mode: proxy::Mode,
    ) -> (std::process::ExitStatus, Vec<u8>, Vec<u8>, Vec<u8>) {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("sub/cargo.log");
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let st = proxy::run(
            &Handoff::new("sh", ["-c", script]),
            &log,
            mode,
            &mut out,
            &mut err,
        )
        .expect("runs");
        (
            st,
            out,
            err,
            std::fs::read(&log).expect("the log is written"),
        )
    }

    #[cfg(unix)]
    fn pty(rows: u16, cols: u16) -> proxy::Mode {
        proxy::Mode::Pty {
            size: libc::winsize {
                ws_row: rows,
                ws_col: cols,
                ws_xpixel: 0,
                ws_ypixel: 0,
            },
            stderr_too: true,
        }
    }

    /// Half two of the guard: what the terminal gets and what the log gets
    /// are the tool's bytes, colour codes and carriage returns included, and
    /// the exit status is the tool's.
    #[cfg(unix)]
    #[test]
    fn stage_five_copies_and_never_filters() {
        let bytes = b"\x1b[33mwarning\x1b[0m: x\r  Building [==> ] 1/2\rdone\n";
        let script = "printf '\\033[33mwarning\\033[0m: x\\r  Building [==> ] 1/2\\rdone\\n'; \
                      printf 'E\\n' >&2; exit 7";
        let (st, out, err, log) = proxied(script, proxy::Mode::Pipes);
        assert_eq!(st.code(), Some(7), "the tool's own exit code");
        assert_eq!(out, bytes, "stdout, unchanged");
        assert_eq!(err, b"E\n", "stderr stays stderr on pipes");
        assert_eq!(log.len(), out.len() + err.len(), "the log holds both");
        assert!(log.windows(bytes.len()).any(|w| w == bytes));
    }

    /// On a pty the tool sees a terminal of the caller's size, which is what
    /// keeps colour and progress bars; the log gets what the terminal got.
    #[cfg(unix)]
    #[test]
    fn on_a_pty_the_tool_sees_a_terminal_of_the_callers_size() {
        let (st, out, _err, log) = proxied(
            "test -t 1 && echo STDOUT-TTY; test -t 2 && echo STDERR-TTY >&2; stty size </dev/tty",
            pty(33, 111),
        );
        let s = String::from_utf8_lossy(&out);
        assert!(st.success(), "{st:?}: {s}");
        assert!(s.contains("STDOUT-TTY"), "{s}");
        assert!(s.contains("STDERR-TTY"), "{s}");
        assert!(s.contains("33 111"), "the caller's size: {s}");
        assert_eq!(log, out, "the log holds the same bytes the terminal got");
    }

    /// A resize of the caller's terminal reaches the tool: the pty takes our
    /// stdout's size (24x80 here, where it is no terminal) and the kernel
    /// sends the tool SIGWINCH.
    #[cfg(unix)]
    #[test]
    fn a_resize_reaches_the_tool() {
        use std::sync::atomic::Ordering;
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("started");
        let script = format!(
            "trap 'stty size </dev/tty; exit 0' WINCH; touch {m}; \
             i=0; while [ $i -lt 400 ]; do sleep 0.05; i=$((i+1)); done; exit 9",
            m = marker.display()
        );
        let h = Handoff::new("sh", ["-c", &script]);
        let t = std::thread::spawn(move || {
            let (mut out, mut err) = (Vec::new(), Vec::new());
            let st = proxy::run(
                &h,
                &dir.path().join("x.log"),
                pty(33, 111),
                &mut out,
                &mut err,
            );
            st.map(|st| (st, out))
        });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while !marker.exists() {
            assert!(
                std::time::Instant::now() < deadline,
                "the tool never started"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        proxy::SIGWINCHES.fetch_add(1, Ordering::SeqCst);
        let (st, out) = t.join().unwrap().expect("runs");
        let s = String::from_utf8_lossy(&out);
        assert_eq!(st.code(), Some(0), "the tool saw SIGWINCH: {s}");
        assert!(s.contains("24 80"), "the new size reached the tool: {s}");
    }

    /// A Ctrl-C to `nros build` reaches the tool's process group, and the
    /// status says the signal (the caller reports 128 + 2).
    #[cfg(unix)]
    #[test]
    fn an_interrupt_is_forwarded_to_the_tools_process_group() {
        use std::sync::atomic::Ordering;
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("started");
        // A foreground grandchild in the tool's process group, as ninja's
        // compilers are, with an argv nothing else on the host has.
        let tag = format!("31.{}", std::process::id());
        let script = format!(
            "touch {m}; sleep {tag}; echo survived",
            m = marker.display()
        );
        let h = Handoff::new("sh", ["-c", &script]);
        let t = std::thread::spawn(move || {
            let (mut out, mut err) = (Vec::new(), Vec::new());
            proxy::run(
                &h,
                &dir.path().join("x.log"),
                pty(24, 80),
                &mut out,
                &mut err,
            )
            .map(|st| (st, dir))
        });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while !marker.exists() {
            assert!(
                std::time::Instant::now() < deadline,
                "the tool never started"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        // What the handler does on a real SIGINT, without signalling the
        // test process.
        proxy::SIGINTS.fetch_add(1, Ordering::SeqCst);
        let (st, _dir) = t.join().unwrap().expect("runs");
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(st.signal(), Some(libc::SIGINT), "{st:?}");
        // No process left whose argv is our sleep's.
        let alive: Vec<String> = std::fs::read_dir("/proc")
            .unwrap()
            .flatten()
            .filter_map(|e| std::fs::read(e.path().join("cmdline")).ok())
            .map(|c| String::from_utf8_lossy(&c).replace('\0', " "))
            .filter(|c| c.starts_with("sleep ") && c.contains(&tag))
            .collect();
        assert!(
            alive.is_empty(),
            "the grandchild outlived the interrupt: {alive:?}"
        );
    }

    #[test]
    fn escalating_offline_drops_exactly_that_flag() {
        // Issue 0873: the retry must differ from the original in
        // ONE way. Anything else and a cold-cache retry would run a
        // command the caller never authorised.
        let h = Handoff::new("cargo", vec!["generate-lockfile", "--offline"]).in_dir("/w");
        let online = h.without_offline().expect("it carries --offline");
        assert_eq!(online.args, vec![OsString::from("generate-lockfile")]);
        assert_eq!(online.cwd, h.cwd, "the retry must run in the same dir");
        assert_eq!(online.env, h.env, "and with the same environment");
        assert_eq!(online.program, h.program);
    }

    /// The jobserver reaches the constructed command — set and removed — and
    /// stays out of what a user is shown.
    #[test]
    fn the_jobserver_reaches_the_command_but_not_the_display() {
        let env: super::super::jobserver::ChildEnv = vec![
            (
                "MAKEFLAGS".into(),
                Some("-j2 --jobserver-auth=fifo:/p".into()),
            ),
            ("CMAKE_BUILD_PARALLEL_LEVEL".into(), None),
        ];
        let h = Handoff::new("cmake", ["--build", "."]).with_jobserver(&env);
        let cmd = h.command();
        let envs: Vec<_> = cmd.get_envs().collect();
        assert!(envs.contains(&(
            std::ffi::OsStr::new("MAKEFLAGS"),
            Some(std::ffi::OsStr::new("-j2 --jobserver-auth=fifo:/p"))
        )));
        assert!(envs.contains(&(std::ffi::OsStr::new("CMAKE_BUILD_PARALLEL_LEVEL"), None)));
        assert_eq!(h.display(), "cmake --build .");
    }

    #[test]
    fn a_step_that_never_asked_for_offline_has_no_retry() {
        // `None` is what stops `run_configure` from re-running an
        // identical command and reporting the second failure as if
        // it were new information.
        let h = Handoff::new("cmake", vec!["--build", "."]);
        assert!(h.without_offline().is_none());
    }

    #[test]
    fn display_round_trips_through_a_shell_split() {
        let argv = [
            "build",
            "--config",
            "env.CFLAGS_x.value=\"-mcpu=cortex-m3 -mthumb -fstack-usage\"",
            "it's",
            "",
            "$HOME",
        ];
        let h = Handoff::new("cargo", argv).with_env("K", "a b");
        let line = h.display();
        // A real POSIX shell splits it back into the same words.
        let out = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("printf '%s\\0' {line}"))
            .output()
            .expect("sh");
        let words: Vec<String> = String::from_utf8(out.stdout)
            .unwrap()
            .split_terminator('\0')
            .map(str::to_string)
            .collect();
        let mut want = vec!["K=a b".to_string(), "cargo".to_string()];
        want.extend(argv.iter().map(|a| (*a).to_string()));
        assert_eq!(words, want, "{line}");
        assert!(line.starts_with("K='a b' cargo "), "{line}");
    }
}
