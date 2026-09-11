//! Where a provisioning step's OUTPUT goes — phase-447 E3 / RFC-0099 D7.
//!
//! `nros setup` installs a plan's packages concurrently, and per-package output
//! must still read in PLAN order (the fourth of the ordered things in
//! `cmd/setup/session.rs`). The session's `OrderedLog` does that for every line
//! the session itself writes; this module is how the lines written BELOW it —
//! `sdk_store`'s own notes, and the stdout/stderr of every `curl`, `tar`, `git`,
//! `configure` and `make` a step spawns — reach the same log instead of the
//! terminal.
//!
//! The mechanism is a thread-local SINK the executor installs on a worker thread
//! for the duration of one step ([`with_step_sink`]). Code that prints asks
//! [`say`]; code that spawns asks [`status`]. With no sink installed — every
//! caller that is not the session executor — both behave exactly as they did:
//! `eprintln!` and an inherited-stdio `Command::status`. So `sdk_store` keeps its
//! signatures, and the three call sites that reach it outside a session do not
//! change behaviour.
//!
//! It also owns the one piece of output a step produces by itself: progress for
//! a LONG download (issue 1266). `curl --silent` made a 1.4 GB fetch look hung
//! for fifteen minutes; [`watch_download`] prints a byte-count line only once a
//! fetch has run for a while, and then periodically — plain lines, because the
//! normal reader is a CI log, where a carriage-return progress bar renders as
//! one very long line.

use std::{
    cell::RefCell,
    io::{BufRead as _, BufReader, Read},
    path::Path,
    process::{Command, ExitStatus, Stdio},
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};

/// Where one step's lines go. `Arc` because a step's output is produced on more
/// than one thread: the worker, the reader threads of a spawned child, and a
/// download's progress watcher.
pub type StepSink = Arc<dyn Fn(String) + Send + Sync>;

thread_local! {
    static STEP_SINK: RefCell<Option<StepSink>> = const { RefCell::new(None) };
}

/// Restores the previous sink when dropped — including on unwind, so a step
/// that panics cannot leave its sink installed for the next step the same
/// worker takes.
struct SinkGuard(Option<StepSink>);

impl Drop for SinkGuard {
    fn drop(&mut self) {
        let prev = self.0.take();
        STEP_SINK.with(|s| *s.borrow_mut() = prev);
    }
}

/// Run `f` with every line it produces routed to `sink`.
pub fn with_step_sink<R>(sink: StepSink, f: impl FnOnce() -> R) -> R {
    let prev = STEP_SINK.with(|s| s.borrow_mut().replace(sink));
    let _guard = SinkGuard(prev);
    f()
}

/// The sink installed on THIS thread, if any.
pub fn current_sink() -> Option<StepSink> {
    STEP_SINK.with(|s| s.borrow().clone())
}

fn say_via(sink: Option<&StepSink>, line: String) {
    match sink {
        Some(s) => s(line),
        None => eprintln!("{line}"),
    }
}

/// Print one line of a step's output: to the step's sink when a session is
/// executing it, else to stderr exactly as before.
pub fn say(line: String) {
    say_via(current_sink().as_ref(), line);
}

/// `Command::status`, with the child's stdout and stderr routed to the step's
/// sink line by line when there is one. Without a sink the child inherits
/// stdio, unchanged.
///
/// Routed output is still LIVE for the step the log is currently streaming —
/// the `OrderedLog` holds only the lines of steps that are ahead of the
/// earliest unfinished one — so a long source build in plan position 0 shows
/// its compiler output as it happens, as it always did.
pub fn status(cmd: &mut Command) -> std::io::Result<ExitStatus> {
    let Some(sink) = current_sink() else {
        return cmd.status();
    };
    let mut child = cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
    let streams: Vec<Box<dyn Read + Send>> = [
        child
            .stdout
            .take()
            .map(|s| Box::new(s) as Box<dyn Read + Send>),
        child
            .stderr
            .take()
            .map(|s| Box::new(s) as Box<dyn Read + Send>),
    ]
    .into_iter()
    .flatten()
    .collect();
    let (done, drained) = mpsc::channel::<()>();
    let readers = streams.len();
    for stream in streams {
        let (sink, done) = (sink.clone(), done.clone());
        // Detached, not scoped: see the drain below.
        std::thread::spawn(move || {
            forward_lines(stream, &sink);
            let _ = done.send(());
        });
    }
    drop(done);
    let status = child.wait()?;
    // A child's output is complete when its pipes close — normally the moment
    // it exits. A BACKGROUND process it started (a build server, a daemon)
    // inherits the pipes and can hold them open for as long as it lives; with
    // inherited stdio that was harmless, and waiting for EOF here would hang
    // the install on it. So the drain is bounded, and says when it gave up.
    let deadline = Instant::now() + DRAIN_GRACE;
    for _ in 0..readers {
        let left = deadline.saturating_duration_since(Instant::now());
        if drained.recv_timeout(left).is_err() {
            sink(
                "    (a background process this step started still holds its output open; \
                 not waiting for it)"
                    .to_string(),
            );
            break;
        }
    }
    Ok(status)
}

/// How long [`status`] keeps reading a child's pipes after it exited. Not a
/// measurement of anything: an ordinary child has closed them by then, and the
/// bound exists only so a process the child left behind cannot hang the step.
const DRAIN_GRACE: Duration = Duration::from_secs(5);

/// Forward a child's stream as lines. A carriage return is a terminal repaint,
/// not a line: keep what the last repaint said, so a tool that draws its own
/// progress bar contributes its final state rather than one enormous line.
fn forward_lines(stream: impl Read, sink: &StepSink) {
    let mut reader = BufReader::new(stream);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_until(b'\n', &mut buf) {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                let text = String::from_utf8_lossy(&buf);
                let text = text.trim_end_matches(['\n', '\r']);
                let text = text.rsplit('\r').find(|t| !t.is_empty()).unwrap_or("");
                sink(text.to_string());
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Progress for a long download (issue 1266).
// ---------------------------------------------------------------------------

/// When a download earns a progress line. Pure, so the policy is tested without
/// a clock: a test that asserted on elapsed time would measure the machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProgressPolicy {
    /// Silent for this long. A fetch that finishes first — the index, a small
    /// tool, a warm mirror — prints nothing at all.
    pub quiet_for: Duration,
    /// Then one line per this interval.
    pub every: Duration,
}

impl ProgressPolicy {
    /// The shipped policy: nothing for 10 s, then a line every 30 s. At the
    /// ~0.8 MB/s issue 1267 measured, a 1.4 GB fetch prints ~60 lines over its
    /// 30 minutes, and anything under ~8 MB prints none.
    pub const DEFAULT: Self = Self {
        quiet_for: Duration::from_secs(10),
        every: Duration::from_secs(30),
    };

    /// Is a line due at `elapsed`, given when the last one was printed?
    pub fn due(&self, elapsed: Duration, last: Option<Duration>) -> bool {
        match last {
            None => elapsed >= self.quiet_for,
            Some(at) => elapsed.saturating_sub(at) >= self.every,
        }
    }
}

/// One progress line: what, how much so far, for how long, and the rate.
///
/// No total and no percentage: a dist row declares a URL and a hash, not a size,
/// and the question an operator is asking is "is it moving", which bytes and a
/// rate answer.
pub fn progress_line(what: &str, bytes: u64, elapsed: Duration, finished: bool) -> String {
    let mb = bytes as f64 / 1_000_000.0;
    let secs = elapsed.as_secs_f64().max(0.001);
    let rate = mb / secs;
    let t = elapsed.as_secs();
    let clock = if t >= 60 {
        format!("{}m{:02}s", t / 60, t % 60)
    } else {
        format!("{t}s")
    };
    let verb = if finished { "fetched" } else { "downloading" };
    format!("    … {verb} {what}: {mb:.1} MB in {clock} ({rate:.2} MB/s)")
}

/// Run `fetch` — which writes `dest` — and print progress for it under
/// `policy`, checking every `tick`. A download that ever printed a progress
/// line also prints a closing one, so the log says where it ended.
///
/// The watcher runs on its own thread and carries the CALLER's sink across, so
/// its lines land in the step's plan-order output like any other.
pub fn watch_download<R>(
    dest: &Path,
    what: &str,
    policy: ProgressPolicy,
    tick: Duration,
    fetch: impl FnOnce() -> R,
) -> R {
    let sink = current_sink();
    let (stop, stopped) = mpsc::channel::<()>();
    std::thread::scope(|s| {
        s.spawn(move || {
            let start = Instant::now();
            let mut last: Option<Duration> = None;
            let size = || std::fs::metadata(dest).map(|m| m.len()).unwrap_or(0);
            loop {
                match stopped.recv_timeout(tick) {
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        let elapsed = start.elapsed();
                        if policy.due(elapsed, last) {
                            say_via(sink.as_ref(), progress_line(what, size(), elapsed, false));
                            last = Some(elapsed);
                        }
                    }
                    // Stopped (or the sender is gone): the fetch returned.
                    _ => {
                        if last.is_some() {
                            say_via(
                                sink.as_ref(),
                                progress_line(what, size(), start.elapsed(), true),
                            );
                        }
                        break;
                    }
                }
            }
        });
        // Owned INSIDE the scope, so a `fetch` that panics drops it while
        // unwinding — before the scope joins the watcher, which would otherwise
        // wait on this channel forever and hang the panic instead of reporting it.
        let stop = stop;
        let result = fetch();
        drop(stop);
        result
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    fn capture() -> (StepSink, Arc<Mutex<Vec<String>>>) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let s = seen.clone();
        (Arc::new(move |l: String| s.lock().unwrap().push(l)), seen)
    }

    /// The policy, at synthetic instants: silent until `quiet_for`, then one
    /// line per `every` — never two inside one interval.
    #[test]
    fn a_short_fetch_is_silent_and_a_long_one_speaks_periodically() {
        let p = ProgressPolicy {
            quiet_for: Duration::from_secs(10),
            every: Duration::from_secs(30),
        };
        let s = Duration::from_secs;
        assert!(!p.due(s(0), None));
        assert!(!p.due(s(9), None), "a fetch under quiet_for prints nothing");
        assert!(p.due(s(10), None));
        assert!(!p.due(s(39), Some(s(10))), "not twice inside one interval");
        assert!(p.due(s(40), Some(s(10))));
        assert_eq!(ProgressPolicy::DEFAULT, p, "the shipped policy is this one");
    }

    #[test]
    fn a_progress_line_names_bytes_time_and_rate() {
        let l = progress_line(
            "zephyr-sdk.tar.xz",
            48_000_000,
            Duration::from_secs(60),
            false,
        );
        assert_eq!(
            l,
            "    … downloading zephyr-sdk.tar.xz: 48.0 MB in 1m00s (0.80 MB/s)"
        );
        assert!(progress_line("x", 1, Duration::from_secs(5), true).contains("fetched x"));
    }

    /// The watcher speaks for a fetch that is still running, into the CALLER's
    /// sink, and closes with a final line. Ordered by handshake, not by time:
    /// the "fetch" does not return until the watcher has spoken.
    #[test]
    fn a_long_download_reports_progress_into_the_steps_sink() {
        let dir = crate::test_support::scratch_dir("step_log_progress");
        let dest = dir.join("big.download");
        let (sink, seen) = capture();
        let policy = ProgressPolicy {
            quiet_for: Duration::ZERO,
            every: Duration::from_secs(3600),
        };
        // The bytes are on disk BEFORE the watcher's first tick can look, so
        // every line it prints reads 2.0 MB — a write inside `fetch` would race
        // the first tick, and a first line of 0.0 MB would then hold the next
        // one back for `every`.
        std::fs::write(&dest, vec![0u8; 2_000_000]).unwrap();
        with_step_sink(sink, || {
            watch_download(&dest, "big", policy, Duration::from_millis(1), || {
                // Deadlock guard only; the assertion is on ORDER, not on time.
                let deadline = Instant::now() + Duration::from_secs(60);
                while !seen.lock().unwrap().iter().any(|l| l.contains("2.0 MB")) {
                    assert!(Instant::now() < deadline, "the watcher never spoke");
                    std::thread::yield_now();
                }
            });
        });
        let lines = seen.lock().unwrap().clone();
        assert!(lines[0].contains("downloading big: 2.0 MB"), "{lines:?}");
        assert!(
            lines.last().unwrap().contains("fetched big: 2.0 MB"),
            "{lines:?}"
        );
    }

    /// A fetch that returns inside `quiet_for` leaves no trace — not even the
    /// closing line, which only a download that spoke owes.
    #[test]
    fn a_fetch_that_finishes_inside_quiet_for_prints_nothing() {
        let dir = crate::test_support::scratch_dir("step_log_quiet");
        let (sink, seen) = capture();
        let policy = ProgressPolicy {
            quiet_for: Duration::from_secs(3600),
            every: Duration::from_secs(3600),
        };
        with_step_sink(sink, || {
            watch_download(&dir.join("x"), "x", policy, Duration::from_millis(1), || {})
        });
        assert!(seen.lock().unwrap().is_empty());
    }

    /// A spawned child's stdout AND stderr reach the sink, as lines, with a
    /// carriage-return repaint collapsed to its final state; without a sink
    /// nothing is captured (the child inherits stdio, as before).
    #[cfg(unix)]
    #[test]
    fn a_childs_output_is_routed_to_the_sink_when_one_is_installed() {
        let (sink, seen) = capture();
        let st = with_step_sink(sink, || {
            status(Command::new("sh").args([
                "-c",
                "echo out-line; echo err-line >&2; printf '10%%\\r50%%\\r100%%\\n'",
            ]))
        })
        .unwrap();
        assert!(st.success());
        let mut lines = seen.lock().unwrap().clone();
        lines.sort();
        assert_eq!(lines, ["100%", "err-line", "out-line"]);
        assert!(
            current_sink().is_none(),
            "the sink is uninstalled after the step"
        );
    }

    /// A child that leaves a BACKGROUND process holding its pipes (a build
    /// server) does not hang the step: the child's own output arrives, and the
    /// drain says it stopped waiting. Asserted on outcome — the note is printed
    /// only on the give-up path, which a 600 s holder always reaches.
    #[cfg(unix)]
    #[test]
    fn a_background_process_holding_the_pipes_does_not_hang_the_step() {
        let dir = crate::test_support::scratch_dir("step_log_daemon");
        let pidfile = dir.join("holder.pid");
        let (sink, seen) = capture();
        let st = with_step_sink(sink, || {
            status(Command::new("sh").args([
                "-c",
                &format!("echo before; sleep 600 & echo $! > {}", pidfile.display()),
            ]))
        })
        .unwrap();
        if let Ok(pid) = std::fs::read_to_string(&pidfile) {
            let _ = Command::new("kill").arg(pid.trim()).status();
        }
        assert!(st.success());
        let lines = seen.lock().unwrap().clone();
        assert!(lines.contains(&"before".to_string()), "{lines:?}");
        assert!(
            lines.iter().any(|l| l.contains("not waiting for it")),
            "{lines:?}"
        );
    }

    /// A `fetch` that panics is REPORTED, not hung: the watcher is released
    /// while the panic unwinds, so the scope can join it.
    #[test]
    fn a_panicking_fetch_does_not_hang_its_progress_watcher() {
        let dir = crate::test_support::scratch_dir("step_log_panic_fetch");
        let policy = ProgressPolicy {
            quiet_for: Duration::from_secs(3600),
            every: Duration::from_secs(3600),
        };
        let r = std::panic::catch_unwind(|| {
            watch_download(
                &dir.join("x"),
                "x",
                policy,
                Duration::from_millis(1),
                || panic!("the fetch fell over"),
            )
        });
        assert!(r.is_err(), "the panic propagates");
    }

    /// A step that panics does not leave its sink behind for the next step on
    /// the same worker.
    #[test]
    fn a_panicking_step_restores_the_previous_sink() {
        let (sink, _seen) = capture();
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            with_step_sink(sink, || panic!("boom"))
        }));
        assert!(r.is_err());
        assert!(current_sink().is_none());
    }
}
