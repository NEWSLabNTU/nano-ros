//! Integration test framework for nros
//!
//! This crate provides fixtures and utilities for testing nros components:
//! - Process management (zenohd, QEMU, Zephyr)
//! - Binary building helpers
//! - Output assertion utilities
//!
//! # Example
//!
//! ```ignore
//! use nros_tests::fixtures::zenohd;
//! use rstest::rstest;
//!
//! #[rstest]
//! fn test_pubsub(zenohd: ZenohRouter) {
//!     // zenohd is automatically started and cleaned up
//! }
//! ```

pub mod alloc;
pub mod checker;
pub mod esp32;
// issue 0526 — LINK the posix C port, do not merely depend on it.
//
// `trigger-test` / `loan-e2e` pull `nros-platform-cffi` with `posix-c-port`, and
// its build script compiles `libnros_platform_posix.a` — the archive that
// DEFINES the ~90 `nros_platform_*` symbols `nros-node`'s wake path calls. But a
// dependency nothing references is a dependency rustc does not link: cargo
// passed `--extern nros_platform_cffi=…rlib` and the `-L` for its OUT_DIR, and
// then emitted no `-l static=nros_platform_posix`, because a build script's
// native-lib directives only apply when the crate that emitted them is actually
// linked. The archive sat in the searched directory, unnamed.
//
// Result: six `undefined symbol: nros_platform_*` and FOUR test binaries that
// could not compile — including `wake_latency_cortex_m3`, the issue-0317 gate,
// which therefore reported nothing rather than failing.
//
// `use … as _` is the reference that forces the link without importing a name.
// Same class as the `force_link_backend!` anchors CLAUDE.md documents for
// backends (issues 0155/0163): the symbol is in the rlib, absent from the link.
#[cfg(any(feature = "trigger-test", feature = "loan-e2e"))]
use nros_platform_cffi as _;

pub mod fixtures;
// RFC-0061 / phase-318 W3 — CI lane selection computed from `matrix`.
pub mod buckets;
pub mod ci_lane;
pub mod interop;
pub mod lane_scope;
pub mod matrix;
pub mod output;
pub mod platform;
// Issue 0470 — cross-process port reservation. The bind-then-close allocators
// handed the same ephemeral port to concurrent tests, so two "unique" XRCE
// agents shared one port and a neighbour's samples arrived in this test's
// subscription as `valid=false`.
pub mod port_lease;
pub mod process;
pub mod qemu;
pub mod ros2;
pub mod ros_env;
pub mod treewalk;
pub mod zephyr;

/// Skip the current test with a reason.
///
/// Panics with a `[SKIPPED]` prefix so that CI tooling and test reports
/// can distinguish skips from real failures. Tests that use this will
/// show as FAILED rather than silently passing when prerequisites are
/// missing.
///
/// Configure nextest to treat `[SKIPPED]` panics as expected failures
/// if desired (via `expected` in `.config/nextest.toml`).
///
/// # Example
///
/// ```ignore
/// #[test]
/// fn test_needs_zenohd() {
///     if !is_zenohd_available() {
///         nros_tests::skip!("zenohd not found");
///     }
///     // ... test code
/// }
/// ```
#[macro_export]
macro_rules! skip {
    ($($arg:tt)*) => {
        panic!("[SKIPPED] {}", format_args!($($arg)*))
    };
}

/// [`skip!`] carrying a machine-readable CLASS — issue 0584.
///
/// "Could not run" is several different facts, and a consumer that cannot tell
/// them apart cannot act on any of them. A sweep's junit held 170 skips of
/// which only 4 could be classified after the fact, because the reason lived as
/// prose inside a panic body and the `<skipped message=…>` that survived held
/// `thread '…' panicked at …` instead.
///
/// The classes, and why they differ:
///
/// * `lane` — this coordinate is not in the running lane. Expected, and the
///   count should match what the lane declares.
/// * `capability` — this HOST cannot run it (no cross toolchain, no docker, no
///   emulator). Expected on a lighter machine, never in full CI.
/// * `resource` — a runtime prerequisite was unavailable (a port, a device, a
///   peer process). Usually worth investigating even though it is not a
///   regression.
///
/// There is deliberately NO `fixture` class: a missing in-lane fixture is not a
/// skip at all, it is a broken promise by the build stage, and
/// `fixtures::binaries` fails hard on it.
///
/// Plain [`skip!`] remains valid and is read as `capability`, which is what the
/// overwhelming majority of its ~500 call sites actually mean.
///
/// A class makes skips COUNTABLE; issue 0584 tracks the half that makes them
/// CHECKABLE — comparing a lane's actual skips against the set it declares, so
/// a surprise skip fails instead of blending into a number nobody reads.
#[macro_export]
macro_rules! skip_class {
    ($class:ident, $($arg:tt)*) => {
        panic!(
            "[SKIPPED:{}] {}",
            stringify!($class),
            format_args!($($arg)*)
        )
    };
}

/// Recognising a skip marker in a captured panic message — issue 0658.
///
/// The ONE Rust spelling. Five matrix aggregators independently wrote
/// `msg.contains("[SKIPPED]")`, which is the BARE marker: `[SKIPPED:lane]` does
/// not contain that substring, so every classed skip [`skip_class!`] produces
/// was filed as a FAILED cell. That turned five lane skips into five tier-2
/// reds, and the junit rewriter could not rescue them because by then the
/// marker sat nested inside an aggregate panic body rather than starting it.
///
/// This mirrors `scripts/test/skip_marker.py`, which does the same job on the
/// junit side. Two languages, but one rule, and both are tested.
pub mod skip_marker {
    /// The marker's invariant prefix. `[SKIPPED]` and `[SKIPPED:<class>]` both
    /// start with it — matching on this is what the five call sites got wrong.
    pub const PREFIX: &str = "[SKIPPED";

    /// The class of the skip this message carries, or `None` if it is a real
    /// failure.
    ///
    /// An unclassed `[SKIPPED]` reads as `"capability"`, matching
    /// [`skip_class!`]'s documented default and the Python side.
    ///
    /// Searches ANYWHERE in the message, deliberately: a captured panic from an
    /// inner cell arrives wrapped in the outer test's own prose, and that
    /// nesting is exactly what defeated the naive check. Callers classifying a
    /// message they know to be a whole panic body should prefer
    /// [`starts_with_skip`].
    pub fn class_in(msg: &str) -> Option<&str> {
        let rest = &msg[msg.find(PREFIX)? + PREFIX.len()..];
        match rest.strip_prefix(':') {
            None => rest.starts_with(']').then_some("capability"),
            Some(tail) => {
                let end = tail.find(']')?;
                let class = &tail[..end];
                (!class.is_empty() && class.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'))
                    .then_some(class)
            }
        }
    }

    /// True when this message is a skip rather than a real failure.
    pub fn is_skip(msg: &str) -> bool {
        class_in(msg).is_some()
    }

    /// True when the message BEGINS with a marker — the stricter form the junit
    /// rewriter applies to a `<failure>` payload, where a real failure may
    /// legitimately quote the word.
    pub fn starts_with_skip(msg: &str) -> bool {
        class_in(msg).is_some() && msg.trim_start().starts_with(PREFIX)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn the_bare_marker_reads_as_capability() {
            assert_eq!(class_in("[SKIPPED] zenohd not found"), Some("capability"));
        }

        #[test]
        fn a_classed_marker_yields_its_class() {
            assert_eq!(class_in("[SKIPPED:lane] out of lane: …"), Some("lane"));
            assert_eq!(class_in("[SKIPPED:resource] no port"), Some("resource"));
        }

        /// Issue 0658 itself. The five aggregators wrote
        /// `msg.contains("[SKIPPED]")`, which is false for BOTH of these.
        #[test]
        fn the_bare_literal_would_have_missed_these() {
            let classed = "[SKIPPED:lane] out of lane: workspace-c-native-robot2";
            assert!(!classed.contains("[SKIPPED]"), "the premise of 0658");
            assert!(is_skip(classed));

            let nested = "entry_matrix: 1 of 15 cell(s) FAILED:\n                            zephyr/c/entry_pubsub: [SKIPPED:lane] out of lane: …";
            assert!(!nested.contains("[SKIPPED]"));
            assert!(is_skip(nested), "a nested classed marker is still a skip");
        }

        /// The nesting the junit rewriter cannot see is exactly what `is_skip`
        /// must see — and `starts_with_skip` must NOT, since that is the
        /// rewriter's own stricter rule.
        #[test]
        fn nesting_separates_the_two_predicates() {
            let nested = "outer prose\n  inner: [SKIPPED] reason";
            assert!(is_skip(nested));
            assert!(!starts_with_skip(nested));
            assert!(starts_with_skip("  [SKIPPED] reason"));
        }

        #[test]
        fn a_real_failure_that_merely_mentions_the_word_is_not_a_skip() {
            assert_eq!(class_in("expected the [SKIPPED prefix, got nothing"), None);
            assert_eq!(class_in("assertion failed: skipped == 0"), None);
            assert_eq!(class_in("[SKIPPED:] empty class"), None);
            assert_eq!(class_in("[SKIPPED:Lane] uppercase is not a class"), None);
            assert_eq!(class_in("[SKIPPEDX] not the marker"), None);
        }
    }
}

use std::{
    io::{BufRead, BufReader},
    net::TcpStream,
    process::{Child, ChildStdout},
    sync::atomic::{AtomicU32, Ordering},
    time::{Duration, Instant},
};

/// Intra-process counter for multiple `unique_domain_id()` calls in one test.
static DOMAIN_SEQ: AtomicU32 = AtomicU32::new(0);

/// Returns a unique ROS domain ID for test isolation.
///
/// Nextest runs each test in a separate process, so the PID is unique across
/// concurrent tests. The low 8 bits hold an intra-process sequence counter
/// for the rare case where one test needs multiple distinct domain IDs.
///
/// This avoids the pitfall of a global `AtomicU32` counter that resets per
/// process — all processes would start at the same value.
pub fn unique_domain_id() -> u32 {
    let pid = std::process::id();
    let seq = DOMAIN_SEQ.fetch_add(1, Ordering::Relaxed);
    (pid << 8) | (seq & 0xFF)
}

/// The modulus every test-domain assigner shares (Rust, shell, C++).
///
/// issue 0703 — 101, not 232, and it is not a style choice. Cyclone (RTPS)
/// derives its ports from the domain arithmetically: `7400 + 250*D` for
/// multicast discovery, `+10 + 2*participantIndex` for unicast. Linux hands out
/// ephemeral ports from 32768, and `7400 + 250*102 = 32900` is inside that
/// range — so from domain 102 up, the port a participant MUST have is one the
/// OS may already have given to another process. The bind fails outright
/// (`ddsi_udp_create_conn: failed to bind to ANY:44900: address in use`), which
/// surfaces as a session that will not open: a test failing for a reason having
/// nothing to do with what it tests. The rate tracks how many ephemeral ports
/// are in use, which is why 0703 was ~2-in-5 inside `just check`, 0-in-4 solo,
/// and on a different test each time. Measured with 32768-34000 held: D=101
/// passes, D=102 and D=103 fail.
///
/// 101 leaves margin for the per-participant offsets
/// (`7400 + 250*101 + 11 + 2*9 = 32679`) and is the range ROS 2 documents as
/// safe on Linux, so a value from here is one a user could legally set by hand.
const TEST_DOMAIN_MAX: u32 = 101;

/// Returns a ROS domain ID in the port-safe 1..=101 range (see
/// [`TEST_DOMAIN_MAX`]), unique among concurrently-running tests.
///
/// Use this for tests that must pass the value to ROS 2 or a DDS backend
/// (especially brokerless RTPS like CycloneDDS, where the UDP ports are derived
/// from the domain ID — two live participants on the same domain collide on the
/// SPDP/user-traffic ports). The wider [`unique_domain_id`] is useful for zenoh
/// keyexpr isolation, but ROS 2/DDS implementations reject domain IDs outside
/// their supported range.
///
/// Allocation prefers nextest's `NEXTEST_TEST_GLOBAL_SLOT` — a slot index that
/// is **guaranteed unique among the tests running concurrently** (0..test-threads,
/// reused only after a test finishes). Deriving the domain from the slot is
/// collision-free between live tests. The previous PID-hash was only
/// collision-*rare*: two test PIDs congruent modulo the range land on the same
/// domain, and under load (intervening PID consumption) that happens often enough
/// to flake (Phase 177.33: `ddsi_udp_create_conn: failed to bind … address in
/// use`). Off nextest (no slot env), fall back to the PID hash.
///
/// `seq` (intra-process) spaces out the rare case of one test needing multiple
/// distinct domains; the `* 64` stride keeps those distinct from each other and,
/// for any realistic `test-threads` (≤ 64), from other slots' first domains.
/// Domains reserved per slot, so a test's Nth allocation cannot land on another
/// LIVE test's first one.
///
/// The most any single test allocates today is 3 (`interop_e2e::interop`); 4
/// leaves headroom without shrinking the collision-free slot count further than
/// it has to.
const DOMAINS_PER_SLOT: u32 = 4;

/// Map (slot, seq) into `1..=TEST_DOMAIN_MAX`, giving each slot its own
/// contiguous block.
///
/// The previous scheme was additive — `(slot + seq * 64) % MAX` — and it was
/// correct only because MAX was 232. Issue 0703 lowered MAX to 101 for port
/// safety and left the stride at 64, which silently broke it: `3 * 64 = 192 ≡ 91
/// (mod 101)`, so slot `s` on its FOURTH allocation lands on slot `s - 10`'s
/// first one. Measured over 24 slots × 4 seq: 14 cross-slot collisions, the
/// first being slot 0 and slot 10 both taking domain 1 — which is precisely the
/// domain-1 hazard issue 0672 recorded as "reachable, not yet observed".
///
/// No additive stride can fix this, and that is arithmetic rather than tuning:
/// keeping 4 seq values clear of a 24-slot band needs 4 × 24 = 96 ≤ 101 of
/// separation, and the best stride mod 101 achieves a minimum gap of 16. So the
/// space is PARTITIONED instead — slot `s` owns `[s*4, s*4+3]`, disjoint by
/// construction, zero collisions up to 25 concurrent slots.
///
/// Beyond 25 slots the blocks wrap and collisions resume. That is not a defect
/// of this function but of the ceiling: 101 usable domains cannot be divided
/// among more than 25 slots four ways. A host running nextest with more than 25
/// test threads gets the same collision-*rare* behaviour the pre-slot PID hash
/// had, and the fix there would be to cap `test-threads`, not to widen a range
/// whose upper bound is set by Linux's ephemeral port floor.
///
/// `seq % DOMAINS_PER_SLOT` rather than `seq`: a process that allocates a fifth
/// domain reuses its own first one, which is safe (it owns both), where letting
/// it run past the block would put it on a neighbour's.
fn domain_in_slot(slot: u32, seq: u32) -> u8 {
    let block = slot.wrapping_mul(DOMAINS_PER_SLOT);
    ((block.wrapping_add(seq % DOMAINS_PER_SLOT) % TEST_DOMAIN_MAX) + 1) as u8
}

/// Is a DDS participant already bound to this domain's discovery port?
///
/// Issue 0707 — RTPS derives its ports from the domain id, so "somebody is on
/// domain d" is answerable locally without joining the bus: SPDP's multicast
/// port is `7400 + 250*d`, and every participant on that domain binds it. Read
/// the kernel's table rather than trying to bind: `SO_REUSEADDR` on a multicast
/// socket means a successful bind proves nothing.
///
/// Local only, deliberately. The orphan this exists to dodge is a process the
/// last run left behind on THIS host (issue 0659's class), and a peek that
/// needed real discovery would have to create the participant it is trying to
/// place.
///
/// Non-Linux, or `/proc` unreadable: answers "not busy", so the assignment is
/// exactly what it was before. A probe that cannot see must not invent.
#[cfg(target_os = "linux")]
fn domain_discovery_port_busy(domain: u8) -> bool {
    let want = 7400u32 + 250 * u32::from(domain);
    for table in ["/proc/net/udp", "/proc/net/udp6"] {
        let Ok(body) = std::fs::read_to_string(table) else {
            continue;
        };
        for line in body.lines().skip(1) {
            // `sl  local_address rem_address …` — local is `HEXADDR:HEXPORT`.
            let Some(local) = line.split_whitespace().nth(1) else {
                continue;
            };
            let Some((_, port_hex)) = local.rsplit_once(':') else {
                continue;
            };
            if u32::from_str_radix(port_hex, 16).ok() == Some(want) {
                return true;
            }
        }
    }
    false
}

#[cfg(not(target_os = "linux"))]
fn domain_discovery_port_busy(_domain: u8) -> bool {
    false
}

/// [`domain_in_slot`], stepping to the next block while the domain is occupied.
///
/// Split out from [`unique_ros_domain_id`] so the stepping is testable without
/// binding real sockets — the probe is the parameter.
///
/// Determinism is preserved where it was worth having: with nothing squatting,
/// the first candidate is free and the result is bit-identical to the old
/// scheme. It moves only in the case where reusing the domain would be wrong,
/// which is the whole disagreement issue 0707 recorded between reproducibility
/// and isolation — this keeps the former until it costs the latter.
///
/// Bounded at 25 attempts (the slot count the partition supports) and then
/// gives up and returns the first candidate: an environment where every domain
/// looks busy is not one this function can fix, and failing to return a domain
/// would break every caller.
fn domain_avoiding_busy(slot: u32, seq: u32, busy: impl Fn(u8) -> bool) -> u8 {
    let first = domain_in_slot(slot, seq);
    if !busy(first) {
        return first;
    }
    let slots = TEST_DOMAIN_MAX / DOMAINS_PER_SLOT;
    for step in 1..=slots {
        let candidate = domain_in_slot(slot.wrapping_add(step), seq);
        if !busy(candidate) {
            return candidate;
        }
    }
    first
}

pub fn unique_ros_domain_id() -> u8 {
    let seq = DOMAIN_SEQ.fetch_add(1, Ordering::Relaxed);
    if let Some(slot) = std::env::var("NEXTEST_TEST_GLOBAL_SLOT")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
    {
        // Issue 0707 — a FILTERED or solo nextest run is always global slot 0,
        // so `0*4 + 0 + 1` made every such run take domain 1. That is the run an
        // engineer does when retesting a red solo (which CLAUDE.md prescribes),
        // i.e. the moment they are most likely to be chasing a ghost is the one
        // guaranteed to reuse the bus that produced it.
        return domain_avoiding_busy(slot, seq, domain_discovery_port_busy);
    }
    let pid = std::process::id();
    domain_avoiding_busy(pid, seq, domain_discovery_port_busy)
}

/// Poll a file descriptor for readability using poll(2).
///
/// Returns `true` if the fd is readable, `false` on timeout.
#[cfg(unix)]
fn poll_readable(fd: std::os::unix::io::RawFd, timeout_ms: i32) -> bool {
    let mut fds = [libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    }];
    // Safety: valid pollfd struct, single element
    let ret = unsafe { libc::poll(fds.as_mut_ptr(), 1, timeout_ms) };
    ret > 0 && (fds[0].revents & libc::POLLIN) != 0
}

/// Error type for test utilities
#[derive(Debug, thiserror::Error)]
pub enum TestError {
    #[error("Process failed to start: {0}")]
    ProcessStart(#[from] std::io::Error),

    #[error("Process failed: {0}")]
    ProcessFailed(String),

    #[error("Timeout waiting for condition")]
    Timeout,

    #[error("Build failed: {0}")]
    BuildFailed(String),

    #[error("Output parsing error: {0}")]
    OutputParse(String),

    /// phase-362 W3 — the ROS router is not on this host.
    ///
    /// A distinct variant, not a `ProcessFailed(String)`, because the honest
    /// verdict for a lane that needs `rmw_zenohd` and cannot find one is SKIP,
    /// not fail — and a caller can only make that distinction if the type
    /// carries it. Issue 0599 is the same rule one level up: a lane that
    /// cannot run must say so rather than report OK.
    #[error("ROS router unavailable: {0}")]
    RouterUnavailable(String),
}

pub type TestResult<T> = Result<T, TestError>;

/// Wait for a TCP port to become available
///
/// # Arguments
/// * `port` - The port number to check
/// * `timeout` - Maximum time to wait
///
/// # Returns
/// `true` if the port is available within the timeout, `false` otherwise
pub fn wait_for_port(port: u16, timeout: Duration) -> bool {
    let start = Instant::now();
    let addr = format!("127.0.0.1:{}", port);

    while start.elapsed() < timeout {
        if TcpStream::connect(&addr).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

/// Wait for a TCP port to become available on a specific address
///
/// Like [`wait_for_port`] but checks a specific IP instead of localhost.
/// Useful for verifying zenohd is reachable on a specific address
/// (e.g., a host-forwarded port or a veth bridge IP).
pub fn wait_for_port_on(addr: &str, port: u16, timeout: Duration) -> bool {
    let start = Instant::now();
    let target = format!("{}:{}", addr, port);

    while start.elapsed() < timeout {
        if TcpStream::connect(&target).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

/// Wait for a specific pattern in process output
///
/// # Arguments
/// * `reader` - A buffered reader from the process stdout
/// * `pattern` - The pattern to search for
/// * `timeout` - Maximum time to wait
///
/// # Returns
/// The matching line if found within timeout
pub fn wait_for_pattern(
    reader: &mut BufReader<ChildStdout>,
    pattern: &str,
    timeout: Duration,
) -> TestResult<String> {
    #[cfg(unix)]
    use std::os::unix::io::AsRawFd;

    let start = Instant::now();
    let mut line = String::new();

    #[cfg(unix)]
    let fd = reader.get_ref().as_raw_fd();

    while start.elapsed() < timeout {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => {
                // EOF — wait for more data via poll(2)
                let remaining = timeout.saturating_sub(start.elapsed());
                #[cfg(unix)]
                {
                    let ms = remaining.as_millis().min(500) as i32;
                    poll_readable(fd, ms);
                }
                #[cfg(not(unix))]
                std::thread::sleep(remaining.min(Duration::from_millis(50)));
                continue;
            }
            Ok(_) => {
                if line.contains(pattern) {
                    return Ok(line);
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                let remaining = timeout.saturating_sub(start.elapsed());
                #[cfg(unix)]
                {
                    let ms = remaining.as_millis().min(500) as i32;
                    poll_readable(fd, ms);
                }
                #[cfg(not(unix))]
                std::thread::sleep(remaining.min(Duration::from_millis(50)));
                continue;
            }
            Err(e) => return Err(TestError::ProcessStart(e)),
        }
    }
    Err(TestError::Timeout)
}

/// Collect all output from a process until it exits or timeout
///
/// # Arguments
/// * `child` - The child process
/// * `timeout` - Maximum time to wait
///
/// # Returns
/// The collected stdout as a string
pub fn collect_output(mut child: Child, timeout: Duration) -> TestResult<String> {
    use std::io::Read;
    #[cfg(unix)]
    use std::os::unix::io::AsRawFd;

    let start = Instant::now();
    let mut output = String::new();

    if let Some(mut stdout) = child.stdout.take() {
        #[cfg(unix)]
        let fd = stdout.as_raw_fd();

        // Set up non-blocking read with timeout
        let mut buffer = [0u8; 4096];
        while start.elapsed() < timeout {
            match stdout.read(&mut buffer) {
                Ok(0) => break, // EOF
                Ok(n) => {
                    output.push_str(&String::from_utf8_lossy(&buffer[..n]));
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    let remaining = timeout.saturating_sub(start.elapsed());
                    #[cfg(unix)]
                    {
                        let ms = remaining.as_millis().min(500) as i32;
                        poll_readable(fd, ms);
                    }
                    #[cfg(not(unix))]
                    std::thread::sleep(remaining.min(Duration::from_millis(50)));
                }
                Err(_) => break,
            }

            // Check if process exited
            if let Ok(Some(_)) = child.try_wait() {
                // Read any remaining output
                let _ = stdout.read_to_string(&mut output);
                break;
            }
        }
    }

    // Ensure process is terminated
    process::kill_process_group(&mut child);

    Ok(output)
}

/// Assert that output contains all specified patterns
///
/// # Arguments
/// * `output` - The output string to check
/// * `patterns` - Patterns that must all be present
///
/// # Panics
/// If any pattern is not found in the output
pub fn assert_output_contains(output: &str, patterns: &[&str]) {
    for pattern in patterns {
        assert!(
            output.contains(pattern),
            "Expected output to contain '{}', but it was not found.\nOutput:\n{}",
            pattern,
            output
        );
    }
}

/// Assert that output contains none of the specified patterns
///
/// # Arguments
/// * `output` - The output string to check
/// * `patterns` - Patterns that must not be present
///
/// # Panics
/// If any pattern is found in the output
pub fn assert_output_excludes(output: &str, patterns: &[&str]) {
    for pattern in patterns {
        assert!(
            !output.contains(pattern),
            "Expected output to NOT contain '{}', but it was found.\nOutput:\n{}",
            pattern,
            output
        );
    }
}

/// Count occurrences of a pattern in output
pub fn count_pattern(output: &str, pattern: &str) -> usize {
    output.matches(pattern).count()
}

/// Highest integer that appears immediately after `pattern` across all lines
/// (e.g. `pattern = "Received:"` over `int32-sink` output → the largest counter
/// value delivered). Returns `None` if no line matches with a parseable integer.
///
/// Used for tier e2e proofs (#158): a publisher that emits a MONOTONIC counter
/// encodes its own timer progress in the payload, so the max delivered value
/// tracks how many times that tier's timer fired — independent of how many
/// individual samples were counted (which zenoh delivery batching / drops
/// distort). Comparing two tiers' max values is a deterministic period-ratio
/// proof where a sample-count heuristic only approximates it.
pub fn max_int_after(output: &str, pattern: &str) -> Option<i64> {
    output
        .lines()
        .filter_map(|line| {
            line.split(pattern)
                .nth(1)
                .and_then(|rest| rest.split_whitespace().next())
                .and_then(|tok| tok.parse::<i64>().ok())
        })
        .max()
}

/// Get the project root directory
pub fn project_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

/// RFC-0070 R5 — the build-cache KIND vocabulary, one definition each.
///
/// A kind used to be a bare string literal at every call site. That is why
/// renaming one was a search over an overloaded word rather than an edit:
/// phase-350 W5 tried to rename `compile-check` and found the token also names
/// the compile-check LANE, the `list-compile-checks` subcommand and three
/// scripts, so a global replace rewrote 43 files and produced
/// `list-compile-check-fixturess`. It was reverted, and this module is the
/// prerequisite that was missing.
///
/// The shell half is `NROS_KIND_*` in `scripts/build/build-root.sh`; the two
/// lists are pinned to each other by `build_root_derivation.sh`, which keeps
/// the literals on its EXPECTED side deliberately — a test that asserts a
/// constant equals itself asserts nothing.
pub mod kind {
    // Fixture trees — `<family>-fixtures` per R5.
    pub const CARGO_FIXTURES: &str = "cargo-fixtures";
    pub const CMAKE_FIXTURES: &str = "cmake-fixtures";
    pub const IDF_FIXTURES: &str = "idf-fixtures";
    pub const WEST_FIXTURES: &str = "west-fixtures";

    /// The compile-check lane's trees. Renamed from `compile-check` to carry
    /// the `-fixtures` suffix R5 requires (2026-08-13) — two edits, this and
    /// the shell twin, which is what the constant was extracted for.
    pub const COMPILE_CHECK: &str = "compile-check-fixtures";

    // Everything else — bare `<family>`, named for what it holds.
    pub const CARGO: &str = "cargo";
    pub const QEMU: &str = "qemu";
    pub const QEMU_ZENOH_PICO: &str = "qemu-zenoh-pico";
    pub const ROS_EDITIONS: &str = "ros-editions";
    pub const TOOLS: &str = "tools";
    pub const XRCE_AGENT: &str = "xrce-agent";
    pub const ZENOHD: &str = "zenohd";
    pub const ZEPHYR_WORKSPACE_BUILDS: &str = "zephyr-workspace-builds";

    /// The espflash-packed ESP32-C3 QEMU flash images. A POSTPROCESS of the
    /// `qemu-esp32-baremetal` cargo rows rather than a row of its own — the
    /// manifest has no shape for "another row's artifact, repacked" — so the
    /// KIND is what the two sides share instead of a row (issue 0535).
    pub const ESP32_QEMU: &str = "esp32-qemu";

    /// The pinned POSIX zenoh staticlib + its generated `zenoh_generic_config.h`,
    /// built by `just build-zenoh-posix-fixture` for the symbol/parity gates.
    /// Lived at the repo root as `target-zenoh-fixture-posix/` until issue 0535
    /// moved it under the one build root (R1).
    pub const ZENOH_FIXTURE_POSIX: &str = "zenoh-fixture-posix";
}

/// RFC-0070 R1 — the ONE build-cache root, Rust side.
///
/// The MIRROR of `nros_build_root` in `scripts/build/build-root.sh`. A test
/// resolver cannot source a bash function, and R3 requires the build, the
/// staleness probe and the resolver to agree on the path, so exactly one Rust
/// mirror exists and every resolver goes through it — a second `join("build/…")`
/// literal is the split R3 forbids. Both halves are pinned to the same expected
/// strings: `packages/testing/nros-tests/tests/build_root_derivation.sh` for the
/// shell, the unit tests below for Rust.
///
/// With `NROS_BUILD_ROOT` unset this is `<repo>/build`, i.e. byte-identical to
/// the `project_root().join("build/…")` literals it replaces (phase-334 W2.b
/// step 2: derivation and callers first, paths later).
pub fn build_root() -> std::path::PathBuf {
    match std::env::var("NROS_BUILD_ROOT") {
        Ok(v) if !v.is_empty() => std::path::PathBuf::from(v.trim_end_matches('/')),
        _ => project_root().join("build"),
    }
}

/// RFC-0070 R2 — `<root>/<kind>/<coordinate>…`, the ONE naming shape.
///
/// `kind` is mandatory (a rootless cache dir is the bug R2 exists to prevent)
/// and empty coordinate parts are skipped, matching `nros_build_dir`.
///
/// ```ignore
/// build_dir(kind::COMPILE_CHECK, &[id])   // <root>/compile-check-fixtures/<id>
/// build_dir(kind::CARGO_FIXTURES, &[])    // <root>/cargo-fixtures
/// ```
pub fn build_dir(kind: &str, coords: &[&str]) -> std::path::PathBuf {
    assert!(
        !kind.is_empty(),
        "build_dir: kind is required (RFC-0070 R2)"
    );
    let mut out = build_root().join(kind);
    for part in coords {
        if !part.is_empty() {
            out = out.join(part);
        }
    }
    out
}

/// The `nros-launch-resolve` helper, by ABSOLUTE path (issue 0285 — never
/// `$PATH`, where a stale `~/.nros/bin` copy shadows the in-tree one).
/// `just setup-launch-resolve` builds it; `None` means it has not been built.
///
/// Lives here because two suites need it now: `multihost_partition_bake` and
/// `native_main_macro_misuse`, the latter since phase-330 W4 made the
/// SystemModel a build artifact and tests that want one have to RESOLVE it
/// (issue 0414). A second private copy would be a second spelling of "where is
/// the resolver".
pub fn launch_resolver_bin() -> Option<std::path::PathBuf> {
    let p =
        project_root().join("packages/cli/nros-launch-resolve/target/release/nros-launch-resolve");
    p.is_file().then_some(p)
}

/// Resolve a tool binary from the `nros setup` shared store
/// (`$NROS_HOME/sdk/<tool>/<version>/bin/<exe>`, else `~/.nros/sdk/...`),
/// mirroring `nros-cli-core`'s `store_root` + `tool_prefix` layout. Returns the
/// first version dir carrying `exe`. Lets the test harness discover tools that
/// `nros setup <board>` installed — without it the resolvers only see the
/// `build/<tool>/` (`just`-built) path or the system PATH.
pub fn nros_store_bin(tool: &str, exe: &str) -> Option<std::path::PathBuf> {
    let root = std::env::var_os("NROS_HOME")
        .map(|h| std::path::PathBuf::from(h).join("sdk"))
        .or_else(|| {
            std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".nros/sdk"))
        })?;
    for entry in std::fs::read_dir(root.join(tool)).ok()?.flatten() {
        let cand = entry.path().join("bin").join(exe);
        if cand.is_file() {
            return Some(cand);
        }
    }
    None
}

/// Resolve the `nros` CLI binary the same way `scripts/build/cargo.sh::nros_cli_bin`
/// does: `$NROS_CLI` (must be executable) → `nros` on `PATH` → `${NROS_HOME:-~/.nros}/bin/nros`.
/// Returns `None` if none resolve. Used by orchestration tests that drive
/// `nros plan` / `nros deploy` without re-implementing the lookup.
pub fn nros_cli_bin_path() -> Option<std::path::PathBuf> {
    if let Some(p) = std::env::var_os("NROS_CLI") {
        let pb = std::path::PathBuf::from(p);
        return pb.is_file().then_some(pb);
    }
    if let Ok(out) = std::process::Command::new("sh")
        .args(["-c", "command -v nros"])
        .output()
        && out.status.success()
    {
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !s.is_empty() {
            return Some(std::path::PathBuf::from(s));
        }
    }
    let home = std::env::var_os("NROS_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".nros")))?;
    let cand = home.join("bin/nros");
    cand.is_file().then_some(cand)
}

/// Skip-or-proceed guard for tests that need the `nros` CLI. Mirrors
/// `require_xrce_agent` / `require_zenohd`: prints an install hint and returns
/// `false` when missing (caller `nros_tests::skip!`), `true` otherwise.
pub fn require_nros_cli() -> bool {
    if nros_cli_bin_path().is_none() {
        eprintln!(
            "Skipping test: nros CLI not found (run `just setup-cli` + `source ./activate.sh`)"
        );
        return false;
    }
    true
}

/// Resolve the PX4-Autopilot tree from env. Checks `$PX4_AUTOPILOT_DIR`
/// first (canonical, used by `just px4 test-sitl` / `.envrc`) then the
/// shorter `$PX4_DIR` alias (Phase 212.H.7 user-spec alias). Returns
/// `Some(path)` only when the path also looks like a PX4 checkout
/// (carries a `Makefile`).
pub fn px4_autopilot_dir() -> Option<std::path::PathBuf> {
    for key in ["PX4_AUTOPILOT_DIR", "PX4_DIR"] {
        if let Ok(d) = std::env::var(key) {
            let p = std::path::PathBuf::from(d);
            if p.join("Makefile").is_file() {
                return Some(p);
            }
        }
    }
    None
}

/// Skip-or-proceed guard for tests that need a PX4-Autopilot checkout
/// reachable via `$PX4_AUTOPILOT_DIR` (or the `$PX4_DIR` alias). Phase
/// 212.H.7.
pub fn require_px4() -> bool {
    if px4_autopilot_dir().is_none() {
        eprintln!(
            "Skipping test: PX4_AUTOPILOT_DIR / PX4_DIR unset or not a PX4 checkout \
             (run `just px4 setup`, load `.envrc`, or point at a PX4-Autopilot tree)"
        );
        return false;
    }
    true
}

/// Read the pinned nightly channel from `tools/rust-toolchain.toml`.
///
/// This is the single source of truth for the nightly used by workspace
/// tooling (fmt, miri, llvm-cov, build-std, emit-stack-sizes). Test
/// fixtures that invoke `cargo +<nightly>` read it from here instead of
/// hardcoding the channel.
pub fn pinned_nightly() -> String {
    let path = project_root().join("tools/rust-toolchain.toml");
    let contents = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {}", path.display(), e));
    for line in contents.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("channel")
            && let Some(eq) = rest.find('=')
        {
            return rest[eq + 1..].trim().trim_matches('"').to_string();
        }
    }
    panic!("no channel = \"...\" line in {}", path.display());
}

#[cfg(test)]
mod tests {
    // ---- issue 0707: the domain assigner steps around an occupied bus -------

    #[test]
    fn a_free_domain_is_the_same_answer_as_before() {
        // The property the old scheme was chosen for. With nothing squatting,
        // probe-and-step must be bit-identical to plain partitioning, or the
        // fix has traded away the reproducibility it promised to keep.
        for slot in 0..30u32 {
            for seq in 0..6u32 {
                assert_eq!(
                    super::domain_avoiding_busy(slot, seq, |_| false),
                    super::domain_in_slot(slot, seq),
                    "slot {slot} seq {seq} moved with nothing to avoid"
                );
            }
        }
    }

    #[test]
    fn an_occupied_domain_is_stepped_over() {
        // The 0707 case exactly: a filtered/solo run is global slot 0, so the
        // first candidate is domain 1 and an orphan is sitting on it.
        let first = super::domain_in_slot(0, 0);
        assert_eq!(first, 1, "the hazard's precondition changed");
        let got = super::domain_avoiding_busy(0, 0, |d| d == first);
        assert_ne!(got, first, "stayed on the occupied domain");
        assert!((1..=super::TEST_DOMAIN_MAX as u8).contains(&got));
    }

    #[test]
    fn every_domain_busy_still_returns_one() {
        // Giving up must yield a domain, not hang or panic: a host where the
        // probe says everything is taken is not something this can fix, and a
        // caller with no domain has nowhere to go.
        assert_eq!(
            super::domain_avoiding_busy(0, 0, |_| true),
            super::domain_in_slot(0, 0)
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn the_probe_sees_a_real_bound_discovery_port() {
        // Both directions against the kernel's own table, because a probe that
        // stopped probing would look exactly like a quiet host.
        //
        // NO domain number is written down here. The first version named two
        // (97 "free", 96 "busy"), and the free half is not this test's to
        // assert: the probe reads `/proc/net/udp`, so it reports ANY bound
        // socket, and on 2026-08-21 an unrelated `python3` on this host held
        // 31650 — domain 97's discovery port — which failed tier 1 with
        // "domain 97 looked busy before anything bound it". The test was
        // describing the host, not the probe.
        //
        // So find a domain by BINDING it, which is the only evidence that it
        // was free, and then drive both directions through that one domain:
        // held => busy, dropped => free.
        let mut acquired = None;
        for domain in 1..=super::TEST_DOMAIN_MAX as u8 {
            let port = 7400u16 + 250 * u16::from(domain);
            if super::domain_discovery_port_busy(domain) {
                continue;
            }
            if let Ok(sock) = std::net::UdpSocket::bind(("0.0.0.0", port)) {
                acquired = Some((domain, port, sock));
                break;
            }
        }
        let Some((domain, port, sock)) = acquired else {
            crate::skip!(
                "every domain in 1..={} has its discovery port bound — the host \
                 has no free bus to test the probe against",
                super::TEST_DOMAIN_MAX
            );
        };

        assert!(
            super::domain_discovery_port_busy(domain),
            "bound {port} (domain {domain}) and the probe did not see it"
        );
        drop(sock);
        // UDP has no TIME_WAIT, so the table entry is gone by the next read.
        assert!(
            !super::domain_discovery_port_busy(domain),
            "released {port} (domain {domain}) and the probe still calls it busy"
        );
    }

    use super::*;

    #[test]
    fn test_project_root() {
        let root = project_root();
        assert!(root.join("Cargo.toml").exists());
        assert!(root.join("packages").exists());
    }

    /// phase-334 W2.b step 2 — the Rust half of "the emitted path did not
    /// change". Every literal this commit deleted is written out here against
    /// the derivation that replaced it; if `build_root`/`build_dir` ever stop
    /// agreeing with the pre-migration spelling, this fails rather than the
    /// resolver silently looking in a tree no builder wrote.
    ///
    /// Skipped (not silently passed — the assertions below would be comparing
    /// the relocated root against the old literal, which is the POINT of
    /// `NROS_BUILD_ROOT`) when the root has been relocated.
    #[test]
    fn build_dirs_match_pre_migration_literals() {
        if std::env::var_os("NROS_BUILD_ROOT").is_some_and(|v| !v.is_empty()) {
            return;
        }
        let root = project_root();
        assert_eq!(build_root(), root.join("build"));

        // scripts/build/compile-check-fixtures.sh + scripts/test/compile-check-stale.sh
        assert_eq!(
            build_dir(kind::COMPILE_CHECK, &[]),
            root.join("build/compile-check-fixtures")
        );
        assert_eq!(
            build_dir(kind::COMPILE_CHECK, &["main_macro_form1"]),
            root.join("build/compile-check-fixtures")
                .join("main_macro_form1")
        );
        assert_eq!(
            build_dir(kind::CMAKE_FIXTURES, &[]),
            root.join("build/cmake-fixtures")
        );
        assert_eq!(
            build_dir(kind::CMAKE_FIXTURES, &["shadowing"]),
            root.join("build/cmake-fixtures").join("shadowing")
        );
        // scripts/build/idf-fixtures.sh / west-fixtures.sh
        assert_eq!(
            build_dir(kind::IDF_FIXTURES, &["esp_idf_bringup"]),
            root.join("build/idf-fixtures").join("esp_idf_bringup")
        );
        assert_eq!(
            build_dir(kind::WEST_FIXTURES, &["west_board_import"]),
            root.join("build/west-fixtures").join("west_board_import")
        );
        // scripts/build/fixtures-target-dir.sh (migrated in step 1)
        assert_eq!(
            build_dir(kind::CARGO_FIXTURES, &["qemu-arm-baremetal"]),
            root.join("build/cargo-fixtures").join("qemu-arm-baremetal")
        );

        // R2 — empty coordinate parts are skipped, as in the shell helper.
        assert_eq!(
            build_dir(kind::CARGO, &["", "x"]),
            build_root().join("cargo").join("x")
        );
    }

    #[test]
    #[should_panic(expected = "kind is required")]
    fn build_dir_rejects_empty_kind() {
        let _ = build_dir("", &["x"]);
    }

    #[test]
    fn test_count_pattern() {
        let output = "[PASS] test1\n[PASS] test2\n[FAIL] test3\n[PASS] test4";
        assert_eq!(count_pattern(output, "[PASS]"), 3);
        assert_eq!(count_pattern(output, "[FAIL]"), 1);
    }

    #[test]
    fn test_assert_output_contains() {
        let output = "Hello world\nTest passed";
        assert_output_contains(output, &["Hello", "passed"]);
    }

    #[test]
    #[should_panic(expected = "Expected output to contain")]
    fn test_assert_output_contains_fails() {
        let output = "Hello world";
        assert_output_contains(output, &["missing"]);
    }

    #[test]
    fn test_unique_domain_id() {
        let id1 = unique_domain_id();
        let id2 = unique_domain_id();
        // PID-based, so non-zero
        assert!(id1 > 0);
        // Sequential calls differ in the low 8 bits (intra-process counter)
        assert_ne!(id1, id2);
        assert_eq!(id2 - id1, 1);
    }

    /// issue 0703 follow-up — the regression the ceiling change shipped.
    ///
    /// Lowering `TEST_DOMAIN_MAX` to 101 left the old additive stride of 64 in
    /// place, and `3 * 64 ≡ 91 (mod 101)` put a slot's fourth allocation on a
    /// live neighbour's first. Nothing caught it because no test asserted the
    /// property the scheme exists to provide — only that a domain was in range.
    ///
    /// 25 slots is the designed bound (`101 / DOMAINS_PER_SLOT`); this asserts
    /// the whole grid inside it is collision-free, which the shipped scheme
    /// fails on 14 pairs.
    #[test]
    fn a_slots_domains_never_land_on_a_live_neighbours() {
        let slots = TEST_DOMAIN_MAX / DOMAINS_PER_SLOT;
        let mut owner = std::collections::HashMap::new();
        for slot in 0..slots {
            for seq in 0..DOMAINS_PER_SLOT {
                let d = domain_in_slot(slot, seq);
                if let Some(&prev) = owner.get(&d) {
                    assert_eq!(
                        prev, slot,
                        "domain {d} is claimed by slot {prev} and slot {slot} — \
                         a live test would share a DDS bus with another"
                    );
                }
                owner.insert(d, slot);
            }
        }
        assert_eq!(
            owner.len() as u32,
            slots * DOMAINS_PER_SLOT,
            "every (slot, seq) inside the bound must own a distinct domain"
        );
    }

    /// The nextest thread cap must equal the partition's slot count.
    ///
    /// Issue 0838. `a_slots_domains_never_land_on_a_live_neighbours` proves the
    /// grid is collision-free *inside the bound*; nothing tied that bound to the
    /// number of slots nextest actually creates. It defaults to the CPU count,
    /// so on this 32-core host slots 25..31 aliased onto slots 0..6 —
    /// deterministically, not as a race: slot 25 takes domains 1..4 alongside
    /// slot 0. `domain_in_slot`'s own doc named the remedy ("cap `test-threads`")
    /// and the cap was never applied.
    ///
    /// Reads the real config file rather than restating the number, because the
    /// whole failure was two files disagreeing about one fact.
    #[test]
    fn domain_partition_matches_the_nextest_cap() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .expect("repo root");
        let cfg = root.join(".config/nextest.toml");
        let text = std::fs::read_to_string(&cfg)
            .unwrap_or_else(|e| panic!("reading {}: {e}", cfg.display()));

        let declared = text
            .lines()
            .map(str::trim)
            .find(|l| l.starts_with("test-threads"))
            .and_then(|l| l.split('=').nth(1))
            .and_then(|v| v.trim().parse::<u32>().ok())
            .unwrap_or_else(|| {
                panic!(
                    "{} declares no `test-threads`. Without it nextest uses the \
                     CPU count, and any host with more than {} cores puts two \
                     live tests on one Cyclone domain (issue 0838).",
                    cfg.display(),
                    TEST_DOMAIN_MAX / DOMAINS_PER_SLOT
                )
            });

        assert_eq!(
            declared,
            TEST_DOMAIN_MAX / DOMAINS_PER_SLOT,
            "`test-threads` in {} must equal TEST_DOMAIN_MAX / DOMAINS_PER_SLOT \
             ({} / {}). Above it the domain blocks wrap and slots alias; below \
             it, capacity is wasted.",
            cfg.display(),
            TEST_DOMAIN_MAX,
            DOMAINS_PER_SLOT
        );
    }

    /// Every domain the assigner can produce must stay port-safe (issue 0703):
    /// `7400 + 250*D` must land below Linux's ephemeral floor of 32768.
    #[test]
    fn a_every_reachable_domain_keeps_its_rtps_ports_out_of_the_ephemeral_range() {
        for slot in 0..1000u32 {
            for seq in 0..8u32 {
                let d = u32::from(domain_in_slot(slot, seq));
                assert!(
                    (1..=TEST_DOMAIN_MAX).contains(&d),
                    "domain {d} out of range"
                );
                let port = 7400 + 250 * d + 11 + 2 * 9;
                assert!(
                    port < 32768,
                    "domain {d} needs RTPS port {port}, inside the ephemeral range"
                );
            }
        }
    }
}
