//! A client's session across a hard router restart.
//!
//! Issue 2779: zenoh-pico reopens a client session when its TCP read reports
//! end-of-stream. A port whose read hands the router's close up as 0 ("nothing
//! yet") or as an error polls the dead socket forever, and the client never
//! reconnects. This is the one sequence that tells the two apart, shared by the
//! per-target reopen cells.
//!
//! The router is KILLED (SIGKILL, no grace), so no zenoh CLOSE reaches the
//! client and only its TCP connection ending says the router is gone. A
//! graceful stop lets the router CLOSE the session, which takes a different
//! path in the client: a Zephyr cell that stopped its router that way passed on
//! the unfixed link too.

use std::time::Duration;

use crate::{TestResult, count_pattern, fixtures::ZenohRouter};

/// How long the router stays down: past a few 1 Hz talker periods, and long
/// enough that the client has read the end-of-stream before the port answers
/// again.
pub const OUTAGE: Duration = Duration::from_secs(5);

/// How long the client gets to reopen and hear a sample once the router is
/// back. zenoh-pico retries the open about once a second and the talker
/// publishes once a second; 60 s covers both on a loaded host or a QEMU guest.
pub const REOPEN_WINDOW: Duration = Duration::from_secs(60);

/// How long the client gets to hear its first sample before the outage.
pub const FIRST_SAMPLE_WINDOW: Duration = Duration::from_secs(60);

/// The output of a process whose session the sequence watches.
pub trait OutputStream {
    /// Whatever the process has printed since the last read, without waiting
    /// and without stopping it.
    fn read_pending(&mut self) -> String;

    /// Read until `pattern` has appeared `count` times in this call's output,
    /// or `timeout` passes. May stop the process at the deadline.
    fn read_until_count(&mut self, pattern: &str, count: usize, timeout: Duration) -> String;
}

/// What the client printed in each phase of the sequence.
pub struct Reopen {
    /// Up to its first sample, before the outage.
    pub before: String,
    /// While the router was down (read just before the restart).
    pub during: String,
    /// After the restart.
    pub after: String,
    /// From the restart to the end of the wait for a sample: the reopen
    /// latency when one was heard, the whole window when none was.
    pub waited: Duration,
}

impl Reopen {
    /// Samples heard before the outage: the reopen can only be judged if this
    /// is at least one.
    pub fn heard_before(&self, sample_marker: &str) -> usize {
        count_pattern(&self.before, sample_marker)
    }

    /// Samples heard after the router came back. Everything printed while it
    /// was down was read off into [`Self::during`], so each one here arrived
    /// over a reopened session.
    pub fn heard_after(&self, sample_marker: &str) -> usize {
        count_pattern(&self.after, sample_marker)
    }

    /// One line for the test log, pass or fail, so a green run carries its
    /// evidence too.
    pub fn summary(&self, sample_marker: &str) -> String {
        format!(
            "heard {} before the outage, {} while the router was down, {} in {:.1} s \
             after the restart",
            self.heard_before(sample_marker),
            count_pattern(&self.during, sample_marker),
            self.heard_after(sample_marker),
            self.waited.as_secs_f64(),
        )
    }

    /// The whole transcript, for a failure message.
    pub fn transcript(&self) -> String {
        format!(
            "--- before the outage ---\n{}\n--- while the router was down ---\n{}\n\
             --- after the restart ---\n{}",
            self.before, self.during, self.after
        )
    }
}

/// Wait for `listener` to hear one sample, kill `router` hard, keep it down
/// for [`OUTAGE`], restart it on the same port, and wait up to
/// [`REOPEN_WINDOW`] for one more sample.
///
/// If no sample arrives before the outage the sequence stops there and
/// returns with empty `during`/`after`; the caller asserts on
/// [`Reopen::heard_before`] first.
pub fn listener_across_a_hard_router_restart(
    router: &mut ZenohRouter,
    listener: &mut dyn OutputStream,
    sample_marker: &str,
) -> TestResult<Reopen> {
    let before = listener.read_until_count(sample_marker, 1, FIRST_SAMPLE_WINDOW);
    if count_pattern(&before, sample_marker) == 0 {
        return Ok(Reopen {
            before,
            during: String::new(),
            after: String::new(),
            waited: Duration::ZERO,
        });
    }
    router.kill_hard();
    std::thread::sleep(OUTAGE);
    let during = listener.read_pending();
    router.restart()?;
    let restarted = std::time::Instant::now();
    let after = listener.read_until_count(sample_marker, 1, REOPEN_WINDOW);
    let waited = restarted.elapsed();
    Ok(Reopen {
        before,
        during,
        after,
        waited,
    })
}

impl OutputStream for crate::qemu::QemuProcess {
    fn read_pending(&mut self) -> String {
        crate::qemu::QemuProcess::read_pending(self)
    }

    fn read_until_count(&mut self, pattern: &str, count: usize, timeout: Duration) -> String {
        self.collect_until_count(pattern, count, timeout)
    }
}

impl OutputStream for crate::process::ManagedProcess {
    fn read_pending(&mut self) -> String {
        // A wait on a pattern nothing prints, short enough to be a read: the
        // managed wait never stops the process at its deadline.
        self.collect_until("\u{0}never printed\u{0}", Duration::from_millis(200))
    }

    fn read_until_count(&mut self, pattern: &str, count: usize, timeout: Duration) -> String {
        // One match per call, accumulated: `ManagedProcess::collect_until_count`
        // returns an EMPTY string when the count is unmet (issue 0670), and the
        // caller here needs the real transcript either way.
        let deadline = std::time::Instant::now() + timeout;
        let mut out = String::new();
        while count_pattern(&out, pattern) < count {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                break;
            }
            let chunk = self.collect_until(pattern, remaining);
            if chunk.is_empty() && !self.is_running() {
                break;
            }
            out.push_str(&chunk);
        }
        out
    }
}
