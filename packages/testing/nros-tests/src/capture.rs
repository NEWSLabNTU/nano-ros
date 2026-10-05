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
pub fn append(capture: &mut String, bytes: &[u8]) {
    capture.push_str(&String::from_utf8_lossy(bytes));
    if capture.len() > CAP_BYTES + SLACK_BYTES {
        compact(capture);
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
}
