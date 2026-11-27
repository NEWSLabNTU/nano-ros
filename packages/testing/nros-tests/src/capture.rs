//! A BOUNDED capture of a child process's console — issue 1697.
//!
//! Every reader in this crate accumulates what a fixture prints into a
//! `String`, so a test can grep it. Until 1697 each one did
//! `output.push_str(&String::from_utf8_lossy(&buf[..n]))` with no limit, and a
//! guest that looped on a log line (issue 1696, Cyclone's receive thread on
//! Zephyr) grew the test process to 91 GB before the kernel killed it — a test
//! failure turned into a host-wide out-of-memory event that took other sessions
//! with it.
//!
//! [`append`] is the one spelling. It keeps the capture a plain `String`, so no
//! caller's pattern search changes, and bounds it:
//!
//! * the first [`HEAD_BYTES`] are kept — boot and the first error are there;
//! * after them, a marker line saying how many bytes were dropped;
//! * then a rolling tail of the most recent output.
//!
//! The whole capture stays under [`CAP_BYTES`] plus one compaction's slack. A
//! search for something the guest printed in the DROPPED middle no longer
//! matches; that costs only a run which already printed megabytes, which is a
//! failure in its own right, and the marker says so in the failure text.
//!
//! Gate: `check-test-capture-bounded` refuses a raw
//! `push_str(&String::from_utf8_lossy(..))` anywhere else in this crate.

/// Bytes kept from the START of the capture.
pub const HEAD_BYTES: usize = 1 << 20;

/// Upper bound of the capture after a compaction.
pub const CAP_BYTES: usize = 8 << 20;

/// How far past [`CAP_BYTES`] a capture may grow before it is compacted again,
/// so a compaction (one copy of the tail) is amortised over this many bytes of
/// new output rather than paid on every chunk.
pub const SLACK_BYTES: usize = 2 << 20;

/// The marker's fixed prefix. A failure message that quotes the capture shows
/// it, and [`dropped_bytes`] reads the count back from it.
pub const TRUNCATION_MARKER: &str = "\n[nros-tests: console truncated, ";
const MARKER_SUFFIX: &str = " bytes dropped]\n";

/// Append a chunk a reader got from a child, keeping `capture` bounded.
///
/// Issue 2825: also the chunk's copy into this process's [`term_tail`], which
/// a nextest kill prints ([`install_term_dump`]).
pub fn append(capture: &mut String, bytes: &[u8]) {
    term_tail::record(bytes);
    append_copy(capture, bytes);
}

/// [`append`] for a SECOND copy of a chunk already appended once (a reader
/// that keeps stdout and stderr apart beside the merged capture), so the
/// kill-time tail holds each byte once.
pub fn append_copy(capture: &mut String, bytes: &[u8]) {
    capture.push_str(&String::from_utf8_lossy(bytes));
    if capture.len() > CAP_BYTES + SLACK_BYTES {
        compact(capture);
    }
}

// Issue 2825.
/// What this test process has read from its fixtures, kept for a nextest kill.
///
/// A test that waits longer than nextest's kill timeout is killed mid-wait,
/// and every reader here prints what it collected only when its wait
/// RETURNS, so the verdict was a bare TIMEOUT with no output: tier-2's
/// `zephyr_cortex_m_cpp_zenoh_pubsub_e2e` waited 90 s under a 60 s kill and
/// left nothing to read. nextest runs each test in its own process, so this
/// process-wide tail IS the test's: every chunk [`append`] takes is copied in
/// (the last [`term_tail::TAIL_BYTES`]), and nextest's SIGTERM, which comes
/// before its SIGKILL, prints it to stderr, where nextest's failure output
/// shows it, and exits 143.
///
/// Whether a test's wait FITS its kill is a separate, static question that
/// nextest 0.9.99 cannot answer (`show-config` resolves no per-test
/// timeout); phase-591 W9 carries it.
pub mod term_tail {
    use std::sync::{Mutex, Once};

    /// How much of the most recent fixture output a kill prints.
    pub const TAIL_BYTES: usize = 64 * 1024;

    /// The header a kill prints before the tail; tests match it.
    pub const DUMP_HEADER: &str = "[nros-tests: SIGTERM (a nextest kill?) while the test was waiting; the last fixture output it had read:]";

    static TAIL: Mutex<Vec<u8>> = Mutex::new(Vec::new());
    static INSTALL: Once = Once::new();

    /// Copy `bytes` into the tail (and install the SIGTERM dump, once).
    pub fn record(bytes: &[u8]) {
        install_term_dump();
        if let Ok(mut t) = TAIL.lock() {
            push(&mut t, bytes);
        }
    }

    /// Append to a tail buffer, trimming it back to [`TAIL_BYTES`] once it
    /// passes twice that (so a trim is amortised over many chunks).
    pub(crate) fn push(t: &mut Vec<u8>, bytes: &[u8]) {
        t.extend_from_slice(bytes);
        if t.len() > 2 * TAIL_BYTES {
            let cut = t.len() - TAIL_BYTES;
            t.drain(..cut);
        }
    }

    /// A tail buffer as text, newest last, at most [`TAIL_BYTES`] bytes: the
    /// lossy conversion can widen a split character, so the bound is applied
    /// after it, at a character boundary.
    pub(crate) fn text(t: &[u8]) -> String {
        let start = t.len().saturating_sub(TAIL_BYTES);
        let s = String::from_utf8_lossy(&t[start..]).into_owned();
        let mut cut = s.len().saturating_sub(TAIL_BYTES);
        while !s.is_char_boundary(cut) {
            cut += 1;
        }
        s[cut..].to_string()
    }

    /// This process's tail as text ([`text`]).
    #[must_use]
    pub fn snapshot() -> String {
        text(&TAIL.lock().map(|t| t.clone()).unwrap_or_default())
    }

    #[cfg(unix)]
    static PIPE_W: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(-1);

    #[cfg(unix)]
    extern "C" fn on_term(_: libc::c_int) {
        // Async-signal-safe: one write(2) wakes the dumping thread.
        let fd = PIPE_W.load(std::sync::atomic::Ordering::Relaxed);
        if fd >= 0 {
            let b = 1u8;
            // SAFETY: a valid fd and a one-byte buffer that outlives the call.
            unsafe { libc::write(fd, (&raw const b).cast(), 1) };
        } else {
            // SAFETY: async-signal-safe, ends the process as SIGTERM's default would.
            unsafe { libc::_exit(143) };
        }
    }

    /// Install the SIGTERM handler: a self-pipe the handler writes to, read
    /// by a thread that prints [`DUMP_HEADER`] and the tail to stderr, then
    /// exits 143 (what a SIGTERM death reports to a shell). Idempotent.
    /// Installed by the first [`record`], so a test that never read a
    /// fixture keeps SIGTERM's default.
    pub fn install_term_dump() {
        #[cfg(unix)]
        INSTALL.call_once(|| {
            let mut fds = [0i32; 2];
            // SAFETY: a two-element array for pipe(2) to fill.
            if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
                return;
            }
            let (r, w) = (fds[0], fds[1]);
            // Close-on-exec: a fixture child must not hold the pipe.
            // SAFETY: fds pipe(2) just returned.
            unsafe {
                libc::fcntl(r, libc::F_SETFD, libc::FD_CLOEXEC);
                libc::fcntl(w, libc::F_SETFD, libc::FD_CLOEXEC);
            }
            PIPE_W.store(w, std::sync::atomic::Ordering::Relaxed);
            let spawned = std::thread::Builder::new()
                .name("nros-term-dump".into())
                .spawn(move || {
                    let mut b = 0u8;
                    // SAFETY: a valid fd and a one-byte buffer.
                    let n = unsafe { libc::read(r, (&raw mut b).cast(), 1) };
                    if n == 1 {
                        let text = snapshot();
                        use std::io::Write as _;
                        let mut err = std::io::stderr().lock();
                        let _ = writeln!(err, "\n{DUMP_HEADER}");
                        let _ = err.write_all(text.as_bytes());
                        let _ = writeln!(err, "\n[nros-tests: end of the fixture output]");
                        let _ = err.flush();
                    }
                    // SAFETY: ends the process with SIGTERM's conventional status.
                    unsafe { libc::_exit(143) };
                });
            if spawned.is_err() {
                return;
            }
            // SAFETY: a zeroed sigaction with a handler of the right type.
            unsafe {
                let mut sa: libc::sigaction = std::mem::zeroed();
                sa.sa_sigaction = on_term as extern "C" fn(libc::c_int) as usize;
                libc::sigemptyset(&raw mut sa.sa_mask);
                sa.sa_flags = libc::SA_RESTART;
                libc::sigaction(libc::SIGTERM, &raw const sa, std::ptr::null_mut());
            }
        });
    }
}

/// How many bytes [`append`] has dropped from `capture` so far; 0 if none.
pub fn dropped_bytes(capture: &str) -> usize {
    let Some(at) = capture.find(TRUNCATION_MARKER) else {
        return 0;
    };
    let rest = &capture[at + TRUNCATION_MARKER.len()..];
    rest.split(MARKER_SUFFIX)
        .next()
        .and_then(|n| n.parse().ok())
        .unwrap_or(0)
}

/// Rebuild `capture` as head + marker + tail, under [`CAP_BYTES`].
fn compact(capture: &mut String) {
    let head_end = floor_char_boundary(capture, HEAD_BYTES);
    // Where the previous compaction's tail starts, so an earlier drop is
    // counted once and its marker is not itself kept as "tail".
    let (already, body_start) = match capture[head_end..].find(TRUNCATION_MARKER) {
        Some(0) => {
            let n = dropped_bytes(capture);
            let after = capture[head_end..]
                .find(MARKER_SUFFIX)
                .map_or(head_end, |m| head_end + m + MARKER_SUFFIX.len());
            (n, after)
        }
        _ => (0, head_end),
    };
    // Room left for the tail once the head and a marker are in.
    let marker_room = TRUNCATION_MARKER.len() + 20 + MARKER_SUFFIX.len();
    let tail_room = CAP_BYTES - head_end - marker_room;
    let tail_start =
        ceil_char_boundary(capture, capture.len().saturating_sub(tail_room)).max(body_start);
    let dropped = already + (tail_start - body_start);
    let mut out = String::with_capacity(CAP_BYTES);
    out.push_str(&capture[..head_end]);
    out.push_str(TRUNCATION_MARKER);
    out.push_str(&dropped.to_string());
    out.push_str(MARKER_SUFFIX);
    out.push_str(&capture[tail_start..]);
    *capture = out;
}

fn floor_char_boundary(s: &str, mut i: usize) -> usize {
    if i >= s.len() {
        return s.len();
    }
    while !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn ceil_char_boundary(s: &str, mut i: usize) -> usize {
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A writer that never stops, driven far past the cap: the capture stays
    /// bounded, keeps its first line, keeps the latest line, and counts what it
    /// dropped exactly.
    #[test]
    fn an_unbounded_writer_leaves_the_capture_bounded() {
        let mut capture = String::new();
        append(&mut capture, b"boot: first error here\n");
        let line = b"os_sockWaitsetWait: select failed\n";
        let mut written = 23usize;
        // 64 MiB of flood, 8x the cap.
        for i in 0..(64 << 20) / line.len() {
            append(&mut capture, line);
            written += line.len();
            assert!(
                capture.len() <= CAP_BYTES + SLACK_BYTES,
                "chunk {i}: capture reached {} B",
                capture.len()
            );
        }
        append(&mut capture, b"last words\n");
        written += 11;
        assert!(capture.starts_with("boot: first error here\n"));
        assert!(capture.ends_with("last words\n"));
        assert!(capture.contains(TRUNCATION_MARKER));
        assert_eq!(
            capture.matches(TRUNCATION_MARKER).count(),
            1,
            "one marker, however many compactions"
        );
        let marker_len = TRUNCATION_MARKER.len()
            + dropped_bytes(&capture).to_string().len()
            + MARKER_SUFFIX.len();
        assert_eq!(
            capture.len() - marker_len + dropped_bytes(&capture),
            written,
            "every byte is either kept or counted as dropped"
        );
    }

    #[test]
    fn a_small_capture_is_untouched() {
        let mut capture = String::new();
        append(&mut capture, b"hello\n");
        append(&mut capture, b"world\n");
        assert_eq!(capture, "hello\nworld\n");
        assert_eq!(dropped_bytes(&capture), 0);
    }

    /// A multi-byte character straddling a cut must not panic.
    #[test]
    fn compaction_respects_char_boundaries() {
        let mut capture = String::new();
        let chunk = "é".repeat(1 << 16);
        for _ in 0..((CAP_BYTES + SLACK_BYTES) / chunk.len() + 4) {
            append(&mut capture, chunk.as_bytes());
        }
        assert!(capture.len() <= CAP_BYTES + SLACK_BYTES);
        assert!(dropped_bytes(&capture) > 0);
    }

    // Issue 2825 -- a SIGTERM mid-wait prints the fixture output read so far.
    // Each test below is also its own SUBJECT: run with `SUBJECT_ENV` set, it
    // reads (or not) and then waits to be killed, in a child process,
    // because the dump ends the process it runs in.
    const SUBJECT_ENV: &str = "NROS_TERM_TAIL_SUBJECT";
    const READ_MARK: &str = "fixture-line-the-dump-must-show";

    /// In the subject child: read (if `read`), say so, wait to be killed.
    /// Never returns: the parent's SIGTERM ends it, and a subject nobody
    /// killed fails rather than passing.
    #[cfg(unix)]
    fn be_subject(read: bool) -> ! {
        if read {
            let mut capture = String::new();
            append(&mut capture, format!("{READ_MARK}\n").as_bytes());
        }
        println!("SUBJECT-READY");
        std::thread::sleep(std::time::Duration::from_secs(30));
        panic!("the subject was never killed");
    }

    /// In the parent: run `test` as the subject child, SIGTERM it once it is
    /// waiting, and return its exit code, signal and stderr.
    #[cfg(unix)]
    fn run_subject(test: &str) -> (Option<i32>, Option<i32>, String) {
        use std::{
            io::{BufRead as _, BufReader},
            os::unix::process::ExitStatusExt as _,
        };
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", test, "--nocapture"])
            .env(SUBJECT_ENV, "1")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let out = child.stdout.take().unwrap();
        let mut lines = BufReader::new(out).lines();
        let ready = lines.any(|l| l.is_ok_and(|l| l.contains("SUBJECT-READY")));
        assert!(ready, "the subject never reached its wait");
        // SAFETY: signalling our own child by its pid.
        unsafe { libc::kill(i32::try_from(child.id()).unwrap(), libc::SIGTERM) };
        let got = child.wait_with_output().unwrap();
        (
            got.status.code(),
            got.status.signal(),
            String::from_utf8_lossy(&got.stderr).into_owned(),
        )
    }

    #[cfg(unix)]
    #[test]
    fn a_sigterm_mid_wait_prints_what_the_test_had_read() {
        if std::env::var_os(SUBJECT_ENV).is_some() {
            be_subject(true);
        }
        let (code, _, err) =
            run_subject("capture::tests::a_sigterm_mid_wait_prints_what_the_test_had_read");
        assert_eq!(code, Some(143), "stderr:\n{err}");
        assert!(
            err.contains(term_tail::DUMP_HEADER),
            "no dump header:\n{err}"
        );
        assert!(
            err.contains(READ_MARK),
            "the read line is missing from the dump:\n{err}"
        );
    }

    // Negative control: a test that read nothing keeps SIGTERM's default, so
    // it dies by the signal with no dump. The dump above is the mechanism's.
    #[cfg(unix)]
    #[test]
    fn a_test_that_read_nothing_keeps_the_default_sigterm() {
        if std::env::var_os(SUBJECT_ENV).is_some() {
            be_subject(false);
        }
        let (code, sig, err) =
            run_subject("capture::tests::a_test_that_read_nothing_keeps_the_default_sigterm");
        assert_eq!((code, sig), (None, Some(libc::SIGTERM)), "stderr:\n{err}");
        assert!(
            !err.contains(term_tail::DUMP_HEADER),
            "dumped anyway:\n{err}"
        );
    }

    // On a local buffer: under `cargo test` every test shares this process's
    // tail, and the compaction tests above append megabytes to it.
    #[test]
    fn the_tail_keeps_the_newest_bytes_only() {
        let mut t = Vec::new();
        term_tail::push(&mut t, &vec![b'a'; 3 * term_tail::TAIL_BYTES]);
        term_tail::push(&mut t, b"END");
        assert!(t.len() <= 2 * term_tail::TAIL_BYTES);
        let s = term_tail::text(&t);
        assert!(s.len() <= term_tail::TAIL_BYTES, "{}", s.len());
        assert!(s.ends_with("END"));
        // A cut through a multi-byte character still stays in bounds.
        let mut u = Vec::new();
        term_tail::push(&mut u, "é".repeat(term_tail::TAIL_BYTES).as_bytes());
        assert!(term_tail::text(&u).len() <= term_tail::TAIL_BYTES);
    }
}
