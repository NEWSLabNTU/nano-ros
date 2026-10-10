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
// Issue 1692 — run a prebuilt entry as a census producer (`$NROS_CENSUS_OUT`).
pub mod census;
pub mod checker;
pub mod dds_isolation;
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
pub mod capture;
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

/// A precondition this test needs is NOT met — the test FAILS.
///
/// **The rule (issue 1758): a test never skips because something it needs is
/// absent.** It fails, naming what is missing and how to provide it. A missing
/// tool, router, SDK, peer or fixture means the run did less than it claimed,
/// and a test that reports anything but red there is the "skip and pass" shape
/// issues 0584 and 1161 measured: `check-required-features-tests` ran 7 of its
/// 20 tests and reported pass, and tier 1 "passed" 61 ROS 2 interop tests that
/// never ran.
///
/// What a run must NOT attempt is decided by its SCOPE, before any probe: the
/// lane's coordinates (`NROS_TEST_COORDS`) and the host capabilities it does
/// not claim (`NROS_TEST_UNCLAIMED`, see [`lane_scope`]). Out of scope is
/// [`lane_skip!`]; in scope and unmet is this macro. There is no third answer.
///
/// The marker is `[UNMET PRECONDITION]`, deliberately NOT `[SKIPPED…]`, so the
/// junit rewrite leaves it a failure and `check-skip-budget` never reads it as
/// a skip.
///
/// # Example
///
/// ```ignore
/// #[test]
/// fn test_needs_qemu() {
///     if !is_qemu_available() {
///         nros_tests::unmet!("qemu-system-arm not on PATH (run `just setup qemu`)");
///     }
///     // ... test code
/// }
/// ```
#[macro_export]
macro_rules! unmet {
    ($($arg:tt)*) => {
        panic!("[UNMET PRECONDITION] {}", format_args!($($arg)*))
    };
}

/// This test is OUTSIDE the running lane's scope — the one legitimate skip.
///
/// Reached only from a scope predicate (the lane's coordinates, or a host
/// capability the lane declares it does not claim), never from a probe of what
/// the host happens to have: a scope is a property of the LANE, fixed before
/// the run, so the same lane skips the same tests on every host.
/// `check-skip-budget` counts these as deselected, and fails a run in which one
/// names a coordinate the run selected.
#[macro_export]
macro_rules! lane_skip {
    ($($arg:tt)*) => {
        panic!("[SKIPPED:lane] {}", format_args!($($arg)*))
    };
}

/// Recognising a skip marker in a captured panic message — issue 0658.
///
/// The ONE Rust spelling. Five matrix aggregators independently wrote
/// `msg.contains("[SKIPPED]")`, which is the BARE marker: `[SKIPPED:lane]` does
/// not contain that substring, so every classed skip `skip_class!` produced
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
    /// An unclassed `[SKIPPED]` reads as `"capability"`, matching the Python
    /// side. Nothing produces one any more — the only skip is [`lane_skip!`]
    /// (issue 1758) — but a marker from an older binary still parses, and
    /// `check-skip-budget` fails any class but `lane`.
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
/// `seq % DOMAINS_PER_SLOT` rather than `seq`: a fifth allocation's FIRST
/// candidate is the process's own first domain, never a neighbour's. Since issue
/// 1762 that candidate is also claimed by the process, so
/// [`domain_avoiding_busy`] steps past it rather than handing the same domain
/// out twice.
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

/// Is a ros2cli DAEMON already bound to this domain's XML-RPC port?
///
/// Issue 1333 — the second half of "is somebody on domain d", and the half
/// [`domain_discovery_port_busy`] structurally cannot answer.
///
/// ros2cli keys its daemon on `ROS_DOMAIN_ID` alone: `ros2cli.daemon.get_port()`
/// is literally `11511 + ROS_DOMAIN_ID`. Nothing else is in that key — in
/// particular the DISCOVERY CONFIGURATION is not, so a daemon serves every later
/// caller on its domain a graph computed under whatever `CYCLONEDDS_URI` /
/// `FASTRTPS_DEFAULT_PROFILES_FILE` / `ZENOH_SESSION_CONFIG_URI` the process
/// that STARTED it happened to hold. Since issue 1009 those are exactly the
/// variables this repo pins the bus with, per process, into a tempdir that is
/// gone by the time a later test inherits the daemon.
///
/// Why the SPDP probe cannot cover this, measured on Humble (issue 1333):
///
/// | daemon's RMW        | SPDP `7400+250*d` | daemon port `11511+d` |
/// | ------------------- | ----------------- | --------------------- |
/// | `rmw_cyclonedds_cpp`| bound             | listening             |
/// | `rmw_fastrtps_cpp`  | bound             | listening             |
/// | `rmw_zenoh_cpp`     | **unbound**       | listening             |
///
/// A zenoh daemon is not an RTPS participant and binds no SPDP port at all, so
/// the discovery probe reports the domain FREE and hands out a domain that
/// already carries a foreign daemon. zenoh is this project's default RMW, which
/// makes the blind spot exactly coincident with the common case.
///
/// This matters more here than for a single-domain user, not less:
/// [`unique_ros_domain_id`] RECYCLES domains, and a daemon lingers for two hours
/// after its last use (`ros2cli.daemon.serve`'s inactivity timeout), so a later
/// test landing on a recycled domain is the expected case rather than a rare one.
///
/// TCP rather than UDP, and loopback only — the daemon binds `127.0.0.1`. Same
/// degradation rule as its sibling: `/proc` unreadable, or not Linux, answers
/// "not busy". A probe that cannot see must not invent.
#[cfg(target_os = "linux")]
fn domain_daemon_port_busy(domain: u8) -> bool {
    let want = 11511u32 + u32::from(domain);
    for table in ["/proc/net/tcp", "/proc/net/tcp6"] {
        let Ok(body) = std::fs::read_to_string(table) else {
            continue;
        };
        for line in body.lines().skip(1) {
            // `sl  local_address rem_address st …` — st 0A is TCP_LISTEN.
            let mut f = line.split_whitespace();
            let (Some(local), Some(_rem), Some(st)) = (f.nth(1), f.next(), f.next()) else {
                continue;
            };
            if !st.eq_ignore_ascii_case("0A") {
                continue;
            }
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
fn domain_daemon_port_busy(_domain: u8) -> bool {
    false
}

/// Is anything at all occupying this domain — a DDS participant, or a ros2cli
/// daemon? The predicate [`unique_ros_domain_id`] actually wants.
///
/// Issue 1333 — the two probes answer different questions and neither implies
/// the other. See [`domain_daemon_port_busy`] for the measured table.
fn domain_busy(domain: u8) -> bool {
    domain_discovery_port_busy(domain) || domain_daemon_port_busy(domain)
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
/// Issue 1762 — the ORDER of the candidates, and the CLAIM, are what keep two
/// concurrent callers apart once stepping starts. The busy probe alone cannot:
/// it reads who has BOUND a port, and neither caller has bound anything yet when
/// the other one probes.
///
/// * Order: the caller's OWN block first (`seq`, `seq+1`, … within its slot),
///   and only then the next slots' blocks. The old order jumped straight to
///   `domain_in_slot(slot + step, seq)`, i.e. the next slot's FIRST choice, so
///   with domains 1 and 5 busy, slot 0 stepped twice and slot 1 once, and both
///   landed on 9 (measured: `Clean domain=9` / `Sigterm domain=9` in one run).
/// * Claim: `claim(d)` must also say yes before `d` is returned. The real one
///   ([`claim_domain`]) takes an exclusive lock on a per-domain file held for
///   the rest of the process, so a second caller ANYWHERE on the host (another
///   slot, another worktree's run, the shell or C++ assigner) reads that domain
///   as taken. That is the tie-breaker the probe could not provide.
///
/// Every domain is a candidate, then it gives up and returns the first one: an
/// environment where every domain looks busy is not one this function can fix,
/// and failing to return a domain would break every caller.
fn domain_avoiding_busy(
    slot: u32,
    seq: u32,
    busy: impl Fn(u8) -> bool,
    mut claim: impl FnMut(u8) -> bool,
) -> u8 {
    let first = domain_in_slot(slot, seq);
    let slots = TEST_DOMAIN_MAX / DOMAINS_PER_SLOT;
    for step in 0..=slots {
        for k in 0..DOMAINS_PER_SLOT {
            let candidate = domain_in_slot(slot.wrapping_add(step), seq.wrapping_add(k));
            if !busy(candidate) && claim(candidate) {
                return candidate;
            }
        }
    }
    first
}

/// Where the per-domain claim files live: ONE directory shared by the three
/// assigners (this one, `nros_test_domain.h`'s C++ `nros_test_domain()` and
/// `ros2_e2e_common.sh`'s `nros_unique_ros_domain_id`), so a claim taken in
/// any language is seen by the other two. `$TMPDIR`, else `/tmp`.
fn domain_claim_dir() -> std::path::PathBuf {
    std::env::temp_dir().join("nros-test-domain-claims")
}

/// Issue 1762 — claim `domain` for the rest of this process, or report that
/// another process holds it.
///
/// An exclusive, non-blocking `flock` on `<claim dir>/<domain>.lock`. The
/// descriptor is kept for the life of the process, so the claim lasts exactly
/// as long as the caller can be on the bus, and the kernel drops it when the
/// process ends however it ends (no stale-claim cleanup to get wrong). `flock`
/// locks belong to the open file DESCRIPTION, so a second claim of the same
/// domain from this same process is refused too: one process asking twice gets
/// two domains.
///
/// When the claim cannot be MADE (no writable temp dir, not Unix), it answers
/// "claimed" and the assignment is what the probe alone gives — the same
/// degradation rule as the probes: what cannot be seen must not be invented.
#[cfg(unix)]
fn claim_domain(domain: u8) -> bool {
    use std::os::unix::io::AsRawFd;
    static CLAIMS: std::sync::Mutex<Vec<std::fs::File>> = std::sync::Mutex::new(Vec::new());

    let dir = domain_claim_dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return true;
    }
    let Ok(file) = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join(format!("{domain}.lock")))
    else {
        return true;
    };
    // SAFETY: flock(2) on a descriptor this function owns and keeps open.
    let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if rc != 0 {
        // EWOULDBLOCK is "somebody holds it". Any other error is a lock we
        // could not take for reasons unrelated to the domain: degrade.
        return std::io::Error::last_os_error().raw_os_error() != Some(libc::EWOULDBLOCK);
    }
    CLAIMS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(file);
    true
}

#[cfg(not(unix))]
fn claim_domain(_domain: u8) -> bool {
    true
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
        return domain_avoiding_busy(slot, seq, domain_busy, claim_domain);
    }
    let pid = std::process::id();
    domain_avoiding_busy(pid, seq, domain_busy, claim_domain)
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

    /// issue 1129 / phase-450 W1 — the fixture for this coordinate was not
    /// built.
    ///
    /// The same rule `RouterUnavailable` states one variant up, applied to the
    /// family it matters most for: the honest verdict for a test whose fixture
    /// was never built is SKIP, and a caller can only make that distinction if
    /// the TYPE carries it. Until this existed the distinction was carried by
    /// PROSE — `BuildFailed(msg)` plus `msg.contains("not prebuilt")` — so
    /// every one of the 260 call sites re-decided it by hand, each wrote its
    /// own wording, and `check-skip-budget` had to grep for the wordings it
    /// knew. It knew 4 of 61.
    ///
    /// Display is deliberately UNCHANGED from the `BuildFailed` it replaces.
    /// This variant exists so the decision can be made on the type; restating
    /// the prose is a separate change with a wider blast radius, and doing
    /// both at once would make neither reviewable.
    #[error("Build failed: {0}")]
    FixtureNotBuilt(String),
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
                    crate::capture::append(&mut output, &buffer[..n]);
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

/// A `[tool.<name>]` pin from `nros-sdk-index.toml` — issue 1546.
///
/// The test-side reader of the pin a store path is CONSTRUCTED from:
/// `<store>/<tool>/<version>[/<subdir>]`. A test never lists the store to pick
/// a version — the store is shared between checkouts and accumulates (issue
/// 0500), the pin is per-checkout, so "the newest there" is as often a sibling
/// checkout's install as ours (`check-sdk-store-not-enumerated`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdkPin {
    /// `version` — the store directory name.
    pub version: String,
    /// `subdir` with `{version}` expanded — the tarball's own top-level
    /// directory inside `<store>/<tool>/<version>`, when the tool has one.
    pub subdir: Option<String>,
}

impl SdkPin {
    /// `<store>/<tool>/<version>[/<subdir>]` under an SDK store root (the
    /// directory that holds `<tool>/`, e.g. `~/.nros/sdk`).
    pub fn dir_under(&self, sdk_store: &std::path::Path, tool: &str) -> std::path::PathBuf {
        let base = sdk_store.join(tool).join(&self.version);
        match &self.subdir {
            Some(sub) => base.join(sub),
            None => base,
        }
    }
}

/// [`SdkPin`] for `tool`, or `None` when the index or the section is absent.
pub fn sdk_pin(tool: &str) -> Option<SdkPin> {
    let text = std::fs::read_to_string(project_root().join("nros-sdk-index.toml")).ok()?;
    sdk_pin_in(&text, tool)
}

fn sdk_pin_in(index_text: &str, tool: &str) -> Option<SdkPin> {
    let index: toml::Table = toml::from_str(index_text).ok()?;
    let entry = index.get("tool")?.get(tool)?;
    let version = entry.get("version")?.as_str()?.to_string();
    let subdir = entry
        .get("subdir")
        .and_then(|v| v.as_str())
        .map(|s| s.replace("{version}", &version));
    Some(SdkPin { version, subdir })
}

#[cfg(test)]
mod sdk_pin_tests {
    use super::*;

    #[test]
    fn the_real_index_pins_the_tools_tests_construct_paths_for() {
        for tool in [
            "riscv-none-elf-gcc",
            "xrce-agent",
            "zephyr-sdk",
            "zephyr-sdk-1-0-1",
        ] {
            let pin = sdk_pin(tool).unwrap_or_else(|| panic!("no [tool.{tool}] pin"));
            assert!(!pin.version.is_empty(), "{tool}: empty version");
        }
        let z = sdk_pin("zephyr-sdk").unwrap();
        assert_eq!(
            z.subdir.as_deref(),
            Some(&*format!("zephyr-sdk-{}", z.version))
        );
    }

    #[test]
    fn a_missing_tool_is_none() {
        assert_eq!(sdk_pin_in("[tool.a]\nversion = \"1\"\n", "b"), None);
        assert_eq!(
            sdk_pin_in("[tool.a]\nversion = \"1\"\n", "a"),
            Some(SdkPin {
                version: "1".into(),
                subdir: None
            })
        );
    }
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
    /// `esp32` cargo rows rather than a row of its own — the
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

/// Is there an interpreter the resolver's Python half can load?
///
/// Issue 0935 / 0914. "No Python on this host" and "the shipped pair is broken"
/// produce the SAME parse error, and a test that cannot tell them apart is
/// worse than none — it is the vacuous-test class `check-no-vacuous-tests`
/// exists for. So a test that needs Python asks this first and fails
/// with `nros_tests::unmet!`, which names the missing interpreter, keeping a
/// genuine break a different FAILURE.
///
/// Deliberately probes the same way `pyload` does — a `python3` that answers —
/// rather than looking for a file, because what matters is that an interpreter
/// runs, not that one is installed somewhere.
#[must_use]
pub fn host_python_available() -> bool {
    std::process::Command::new("python3")
        .arg("-c")
        .arg("import sys; sys.exit(0)")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Did a probe invocation show that a tool can RUN — not merely that it exists?
///
/// `Command::status()` is `Err` only when the process cannot be SPAWNED, so a
/// bare `.status().is_ok()` answers "present" for a binary that exists and
/// cannot run: a missing shared library (the loader exits 127), an exec failure
/// (126), a crash on start (a signal). Live-peer run 34497290149 had that shape:
/// the XRCE Agent probe said "available", the tests went past their guard, and
/// seven live cells scored REAL failures 0.15 s in — an environment gap reported
/// as a regression.
///
/// Any other exit status counts as "runs": plenty of tools answer a probe with a
/// non-zero USAGE exit, and `MicroXRCEAgent --help` exits 1 on a HEALTHY agent
/// (measured on both the ROS-paired build and the SDK-store copy). So this is
/// deliberately not `.success()`, which would skip every XRCE cell forever.
#[must_use]
pub fn probe_ran(result: std::io::Result<std::process::ExitStatus>) -> bool {
    match result {
        Err(_) => false,
        Ok(status) => !matches!(status.code(), None | Some(126) | Some(127)),
    }
}

#[cfg(test)]
mod probe_ran_tests {
    use super::probe_ran;
    use std::process::Command;

    fn sh(script: &str) -> bool {
        probe_ran(Command::new("sh").arg("-c").arg(script).status())
    }

    #[test]
    fn a_clean_exit_runs() {
        assert!(sh("exit 0"));
    }

    #[test]
    fn a_usage_exit_still_runs() {
        assert!(sh("exit 1"), "a healthy `MicroXRCEAgent --help` exits 1");
    }

    #[test]
    fn a_loader_failure_does_not_run() {
        assert!(!sh("exit 127"));
    }

    #[test]
    fn an_exec_failure_does_not_run() {
        assert!(!sh("exit 126"));
    }

    #[test]
    fn death_by_signal_does_not_run() {
        assert!(!sh("kill -9 $$"));
    }

    #[test]
    fn a_binary_that_cannot_be_spawned_does_not_run() {
        assert!(!probe_ran(
            Command::new("nros-tests-no-such-binary-xyzzy").status()
        ));
    }
}

/// The `nros-launch-resolve` helper, by ABSOLUTE path (issue 0285 — never
/// `$PATH`, where a stale `~/.nros/bin` copy shadows the in-tree one).
/// `just setup-launch-resolve` builds it; `None` means it has not been built.
///
/// Lives here so there is one spelling of "where is the resolver" — a second
/// private copy would be a second answer. (`native_main_macro_misuse` was the
/// other caller until issue 1620 moved its compiles, and the resolve that came
/// with them, into the build stage.)
pub fn launch_resolver_bin() -> Option<std::path::PathBuf> {
    let p =
        project_root().join("packages/cli/nros-launch-resolve/target/release/nros-launch-resolve");
    p.is_file().then_some(p)
}

/// The nano-ros store root — `nros_build_paths::store`, the one spelling
/// (RFC-0103 D6). A retired root variable panics the test with its
/// replacement named, rather than probing a store nobody chose.
pub fn store_root() -> std::path::PathBuf {
    nros_build_paths::store::root()
}

/// Resolve a tool binary from the `nros setup` shared store
/// (`$NROS_HOME/sdk/<tool>/<version>/bin/<exe>`, else `~/.nros/sdk/...`),
/// mirroring `nros-cli-core`'s `store_root` + `tool_prefix` layout. Returns the
/// first version dir carrying `exe`. Lets the test harness discover tools that
/// `nros setup <board>` installed — without it the resolvers only see the
/// `build/<tool>/` (`just`-built) path or the system PATH.
pub fn nros_store_bin(tool: &str, exe: &str) -> Option<std::path::PathBuf> {
    let root = store_root().join("sdk");
    for entry in std::fs::read_dir(root.join(tool)).ok()?.flatten() {
        let cand = entry.path().join("bin").join(exe);
        if cand.is_file() {
            assert_store_bin_loadable(tool, &cand);
            return Some(cand);
        }
    }
    None
}

/// A provisioned tool must be able to LOAD, not merely exist.
///
/// The store's dists link some libraries dynamically against the host — QEMU
/// links `libslirp.so.0`, which stock Ubuntu does not ship. `nros-sdk-index.toml`
/// already declares that (`[tool.qemu] system = ["libslirp"]`, probed via
/// `[system.libslirp] check.sharedlib`) and says why in its own comment:
/// "Declared so setup/doctor can say so BEFORE the smoke check fails with a
/// bare loader error."
///
/// It could not do that here. This resolver returned the path and the first
/// thing to notice was the dynamic loader, so the failure arrived as
/// `error while loading shared libraries` from a tool nobody had asked about —
/// with the diagnosis sitting in the index, unread. The declaration was fine;
/// nothing consulted it on the path where it mattered.
///
/// Checked once per resolved binary and cached, so repeated lookups in one test
/// process cost a single `ldd`.
///
/// `ldd` absent (or not meaningful, as on macOS) is NOT a failure: the check
/// reports nothing rather than inventing a verdict it cannot support.
fn assert_store_bin_loadable(tool: &str, bin: &std::path::Path) {
    use std::sync::{Mutex, OnceLock};
    static SEEN: OnceLock<Mutex<std::collections::HashSet<std::path::PathBuf>>> = OnceLock::new();
    let seen = SEEN.get_or_init(|| Mutex::new(std::collections::HashSet::new()));
    if let Ok(mut s) = seen.lock()
        && !s.insert(bin.to_path_buf())
    {
        return;
    }

    let Ok(out) = std::process::Command::new("ldd").arg(bin).output() else {
        return; // no `ldd` here — report nothing rather than guess
    };
    if !out.status.success() {
        return;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let missing: Vec<&str> = text
        .lines()
        .filter(|l| l.contains("not found"))
        .filter_map(|l| l.split_whitespace().next())
        .collect();
    assert!(
        missing.is_empty(),
        "the provisioned `{tool}` at {} cannot load: {} missing.\n\
         This is an OS package, not part of the SDK dist. The index declares it \
         (`[tool.{tool}] system = [..]`, probed by `[system.*].check`), so:\n\
         \n    nros setup --system --check     # names the missing key\n    \
         nros setup --system                # prints the install command\n\
         \n\
         Without this you would have seen only the loader's \
         `error while loading shared libraries`, with no mention of the \
         package or of `nros setup`.",
        bin.display(),
        missing.join(", "),
    );
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
    let cand = store_root().join("bin/nros");
    cand.is_file().then_some(cand)
}

/// Precondition guard for tests that need the `nros` CLI. Mirrors
/// `require_xrce_agent` / `require_zenohd`: fails with `nros_tests::unmet!`
/// and an install hint when it is missing (issue 1758).
pub fn require_nros_cli() {
    if nros_cli_bin_path().is_none() {
        crate::unmet!("nros CLI not found (run `just setup-cli` + `source ./activate.sh`)");
    }
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
pub fn require_px4() {
    if px4_autopilot_dir().is_none() {
        crate::unmet!(
            "PX4_AUTOPILOT_DIR / PX4_DIR unset or not a PX4 checkout \
             (run `just px4 setup`, load `.envrc`, or point at a PX4-Autopilot tree)"
        );
    }
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
                    super::domain_avoiding_busy(slot, seq, |_| false, |_| true),
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
        let got = super::domain_avoiding_busy(0, 0, |d| d == first, |_| true);
        assert_ne!(got, first, "stayed on the occupied domain");
        assert!((1..=super::TEST_DOMAIN_MAX as u8).contains(&got));
    }

    // ---- issue 1333: a ros2cli daemon occupies a domain too ----------------

    /// The reproduction, with no ROS 2 install required.
    ///
    /// A ros2cli daemon is just a TCP listener on `127.0.0.1:11511+domain`
    /// (`ros2cli.daemon.get_port()`), so binding that port IS the hazard as far
    /// as the probe is concerned — and binding it ourselves makes the test
    /// deterministic instead of dependent on a daemon somebody left running.
    ///
    /// Negative control is the same domain a moment earlier: the assertion is
    /// that the probe CHANGES its answer when the port is taken, not merely
    /// that it says "busy" (a probe stuck at `true` would pass the second half
    /// alone, and that is the failure mode issue 1043 records for this shape).
    #[test]
    fn a_bound_daemon_port_makes_the_domain_busy() {
        use std::net::TcpListener;

        // Find a domain whose daemon port is genuinely free right now, so a
        // daemon a previous run left behind cannot make this vacuous.
        let Some(domain) = (1..=super::TEST_DOMAIN_MAX as u8).find(|d| {
            !super::domain_daemon_port_busy(*d) && !super::domain_discovery_port_busy(*d)
        }) else {
            crate::unmet!(
                "every test domain's daemon port is already bound; nothing free \
                 to measure against"
            );
        };

        // Negative control FIRST — the probe must be capable of saying "free".
        assert!(
            !super::domain_busy(domain),
            "domain {domain} was picked for being free and did not read free"
        );

        let listener = TcpListener::bind(("127.0.0.1", 11511 + u16::from(domain)))
            .expect("could not bind the daemon port the probe is about to read");

        assert!(
            super::domain_daemon_port_busy(domain),
            "a LISTENING socket on 127.0.0.1:{} was invisible to the probe — \
             this is the zenoh case, where no SPDP port is ever bound and the \
             daemon port is the only evidence there is",
            11511 + u16::from(domain)
        );
        assert!(
            super::domain_busy(domain),
            "domain_daemon_port_busy saw it but domain_busy did not — the \
             allocator consults the latter, so only that one matters"
        );

        // And the allocator must actually move off it.
        let stepped =
            super::domain_avoiding_busy(0, 0, |d| d == domain || super::domain_busy(d), |_| true);
        assert_ne!(
            stepped, domain,
            "the allocator handed out a domain carrying a foreign daemon"
        );

        drop(listener);
    }

    #[test]
    fn every_domain_busy_still_returns_one() {
        // Giving up must yield a domain, not hang or panic: a host where the
        // probe says everything is taken is not something this can fix, and a
        // caller with no domain has nowhere to go.
        assert_eq!(
            super::domain_avoiding_busy(0, 0, |_| true, |_| true),
            super::domain_in_slot(0, 0)
        );
    }

    // ---- issue 1762: concurrent callers must not converge once they step ----

    /// The measured case: slots 0 and 1 in one run, their first candidates
    /// (domains 1 and 5) busy, two threads asking at once. Before the fix both
    /// stepped to slot 2's first choice and got domain 9. The claim is a SHARED
    /// fake registry here, the in-process analogue of the lock files.
    #[test]
    fn two_concurrent_callers_whose_first_choices_are_busy_get_distinct_domains() {
        use std::{
            collections::HashSet,
            sync::{Arc, Barrier, Mutex},
        };
        assert_eq!(
            (super::domain_in_slot(0, 0), super::domain_in_slot(1, 0)),
            (1, 5),
            "the reproduction's precondition changed"
        );
        let first_choices_busy = |d: u8| d == 1 || d == 5;
        let claimed: Arc<Mutex<HashSet<u8>>> = Arc::default();
        let barrier = Arc::new(Barrier::new(2));
        let handles: Vec<_> = [0u32, 1]
            .into_iter()
            .map(|slot| {
                let claimed = Arc::clone(&claimed);
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    super::domain_avoiding_busy(slot, 0, first_choices_busy, |d| {
                        claimed.lock().unwrap().insert(d)
                    })
                })
            })
            .collect();
        let got: Vec<u8> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        assert_ne!(
            got[0], got[1],
            "slots 0 and 1 were handed the SAME domain {} (issue 1762)",
            got[0]
        );
        assert!(
            got.iter().all(|d| !first_choices_busy(*d)),
            "an answer is a busy domain: {got:?}"
        );
    }

    /// The ORDER half on its own, with no claim to rescue it: a busy first
    /// choice moves within the caller's own block, so slot 0 and slot 1 stay in
    /// their own blocks (2 and 6) rather than both reaching slot 2's 9.
    #[test]
    fn a_busy_first_choice_steps_within_the_callers_own_block() {
        let busy = |d: u8| d == 1 || d == 5;
        let a = super::domain_avoiding_busy(0, 0, busy, |_| true);
        let b = super::domain_avoiding_busy(1, 0, busy, |_| true);
        assert_eq!(
            (a, b),
            (2, 6),
            "a busy first choice left the caller's block"
        );
    }

    /// The CLAIM half on its own: both slots' whole blocks are busy, so both
    /// must leave them, and both reach the same next block. Order alone gives
    /// them the same answer there; only the claim keeps them apart.
    #[test]
    fn a_claimed_domain_is_not_handed_out_again() {
        use std::collections::HashSet;
        let busy = |d: u8| (1..=8).contains(&d);
        let mut claimed = HashSet::new();
        let a = super::domain_avoiding_busy(0, 0, busy, |d| claimed.insert(d));
        let b = super::domain_avoiding_busy(1, 0, busy, |d| claimed.insert(d));
        assert_ne!(a, b, "two callers leaving busy blocks converged on {a}");
    }

    /// The REAL claim: a domain claimed by this process is refused to a second
    /// claim (flock locks belong to the open file description, so this holds
    /// within one process exactly as across two), and a fresh domain is not.
    #[cfg(unix)]
    #[test]
    fn the_lock_file_claim_refuses_a_second_holder() {
        // A domain nobody on this host has claimed yet, found by claiming it.
        let Some(domain) = (1..=super::TEST_DOMAIN_MAX as u8).find(|d| super::claim_domain(*d))
        else {
            crate::unmet!("every test domain is already claimed on this host");
        };
        assert!(
            !super::claim_domain(domain),
            "domain {domain} was claimed and a second claim still succeeded"
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
            crate::unmet!(
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
        // scripts/build/west-fixtures.sh
        assert_eq!(
            build_dir(kind::WEST_FIXTURES, &["west_board_import"]),
            root.join("build/west-fixtures").join("west_board_import")
        );
        // scripts/build/fixtures-target-dir.sh (migrated in step 1)
        assert_eq!(
            build_dir(kind::CARGO_FIXTURES, &["baremetal"]),
            root.join("build/cargo-fixtures").join("baremetal")
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

/// `nros::Executor` is re-exported behind the `nros` crate's own `rmw-cffi`
/// feature (the type needs an active transport backend), and in THIS crate that
/// arrives via `component-runtime-test`, not via `trigger-test` — which enables
/// `nros-node/rmw-cffi` and leaves the umbrella's re-export gated. Gated on the
/// feature that actually brings the type in rather than the one whose name
/// matches, which cost two builds to find out.
#[cfg(all(test, feature = "component-runtime-test"))]
mod w4_reachability {
    /// phase-381 W4 — the graph entry points are reachable through the PUBLIC
    /// façade, not merely present on an internal type.
    ///
    /// Compile-only: it takes the method as a function value, which fails to
    /// build if the method is missing, private, or on a type `nros::Executor`
    /// does not alias. Nothing is called, so no session or router is needed.
    #[test]
    fn graph_entry_points_exist_on_the_public_executor() {
        let _ = nros::Executor::get_node_names;
        let _ = nros::Executor::get_topic_names_and_types;
        let _ = nros::Executor::get_service_names_and_types;
        let _ = nros::Executor::count_publishers;
        let _ = nros::Executor::count_subscribers;
        // phase-381 W3/W4 — the six per-node and per-topic forms.
        //
        // `get_subscription_names_and_types_by_node`, NOT `subscriber`: the
        // Rust surface takes rclrs's vocabulary. If someone "fixes" this to
        // match the C spelling, this line stops compiling — which is the point.
        let _ = nros::Executor::get_publisher_names_and_types_by_node;
        let _ = nros::Executor::get_subscription_names_and_types_by_node;
        let _ = nros::Executor::get_service_names_and_types_by_node;
        let _ = nros::Executor::get_client_names_and_types_by_node;
        let _ = nros::Executor::get_publishers_info_by_topic;
        let _ = nros::Executor::get_subscriptions_info_by_topic;
    }
}
