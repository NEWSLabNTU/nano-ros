//! A Zephyr zenoh session open ends at its connect budget (issue 2389).
//!
//! **Bucket (phase-329 W4): matrix consumer.** The one
//! `(ZephyrNativeSim, Rust, Zenoh, ConnectBudget, Example)` cell of
//! `matrix::CELLS`.
//!
//! Issue 2207's contract is that a client connect gives up at its budget. On
//! the qemu_cortex_a53 SMP image (issue 2389) a boot stayed silent for 80 s
//! past a 10 s budget. The open has two waits: the TCP connect, and the zenoh
//! handshake read (InitSyn -> InitAck, OpenSyn -> OpenAck). The connect budget
//! was checked only BETWEEN attempts, so one attempt against a peer that
//! accepts the TCP connect and never answers ran its handshake's own deadline
//! (`Z_TRANSPORT_CONNECT_TIMEOUT`, 10 s) whatever the budget said. Measured on
//! this image before the fix: 10.1 s from the accept to `ConnectionFailed`
//! under a 3 s budget, with no line naming the budget.
//!
//! The image is `examples/zephyr/rust/talker`'s `[image.zephyr_connect_budget]`
//! (`connect_timeout_ms = 3000`). Each case boots it with its own locator
//! (`-testargs --nros-locator=`) and asserts the open ended in
//! `ConnectionFailed` with the budget line, within the budget plus slack, and
//! not before the budget was spent:
//!
//! - nobody listens on the port: every connect is refused, and zenoh-pico
//!   retries a refused connect until the budget runs out;
//! - a listener accepts the TCP connect and never sends a byte: the handshake
//!   read is the wait, and the budget has to bound it.
//!
//! ## Prerequisites
//!
//! - `just zephyr build-fixtures` (the west leaves lane builds
//!   `build-rust-talker-connect-budget-zenoh`)
//!
//! Run with: `cargo nextest run -p nros-tests --test zephyr_connect_budget`

use nros_tests::{
    fixtures::{RequireFixture, build_zephyr_rust_connect_budget},
    matrix::PlatformId,
    output::{SESSION_OPEN_FAILED_MARKER, ZENOH_OPEN_BUDGET_KEY, ZENOH_OPEN_BUDGET_SPENT_PREFIX},
    zephyr::{ZephyrPlatform, ZephyrProcess},
};
use std::{
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

/// The image's `[image.zephyr_connect_budget.knobs.zenoh.limits]
/// connect_timeout_ms`.
const BUDGET: Duration = Duration::from_millis(3000);

/// What the open may overrun its budget by: the last receive under
/// `SO_RCVTIMEO` (`CONFIG_NROS_ZENOH_SOCKET_TIMEOUT_MS`, 100 ms), the last
/// backoff tick, and the console. Measured 30 ms over; the 10 s handshake
/// deadline the budget did not bound is 7 s over.
const SLACK: Duration = Duration::from_millis(1500);

/// How early the open may give up. zenoh-pico reads its clock at the start of
/// the open, a few ms after the connect this test sees, so the open can end a
/// little before the host's own count of the budget.
const EARLY: Duration = Duration::from_millis(250);

/// The NSOS network wait (`nros_platform_zephyr_wait_network`) sleeps 2 s
/// before the open starts. Only the no-listener case measures from the spawn,
/// since nothing marks the open's start there.
const BOOT_BEFORE_OPEN: Duration = Duration::from_millis(2000);

/// Long enough for the unbounded handshake to show as a timing failure rather
/// than a missing line: 10 s handshake deadline plus boot.
const WAIT: Duration = Duration::from_secs(30);

fn image() -> PathBuf {
    nros_tests::fixtures::lane::require_platform_in_lane(
        &[PlatformId::ZephyrNativeSim],
        "the Zephyr connect-budget cell",
    );
    build_zephyr_rust_connect_budget()
        .require("zephyr/rust/talker for native_sim (image zephyr_connect_budget)")
}

/// The budget line and `ConnectionFailed`, both, and the locator the open
/// dialled.
fn assert_gave_up_naming_the_budget(out: &str, locator: &str) {
    let line = out
        .lines()
        .find(|l| l.contains(ZENOH_OPEN_BUDGET_SPENT_PREFIX))
        .unwrap_or_else(|| {
            panic!(
                "the open never said it gave up (`{ZENOH_OPEN_BUDGET_SPENT_PREFIX}`). Output:\n{out}"
            )
        });
    let budget = format!("{ZENOH_OPEN_BUDGET_KEY}{}", BUDGET.as_millis());
    assert!(
        line.contains(locator) && line.contains(&budget),
        "the line must name the locator `{locator}` and `{budget}`: {line}"
    );
    assert!(
        out.contains(SESSION_OPEN_FAILED_MARKER) && out.contains("ConnectionFailed"),
        "the open must end in `{SESSION_OPEN_FAILED_MARKER}` ... `ConnectionFailed`. Output:\n{out}"
    );
}

/// Boot the image against `locator` and return the output once the budget
/// line has printed, with the instant it was seen.
fn boot_until_given_up(bin: &Path, locator: &str) -> (String, Instant, Instant) {
    let spawned = Instant::now();
    let zephyr = ZephyrProcess::start_with_locator(bin, ZephyrPlatform::NativeSim, locator)
        .expect("failed to start the native_sim image");
    let out = zephyr.wait_for_pattern(ZENOH_OPEN_BUDGET_SPENT_PREFIX, WAIT);
    let seen = Instant::now();
    (out, spawned, seen)
}

#[test]
fn zephyr_zenoh_open_against_no_listener_ends_at_its_budget() {
    let bin = image();
    // A port nothing listens on: bind one, read it, close it.
    let port = TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .expect("bind an ephemeral port")
        .port();
    let locator = format!("tcp/127.0.0.1:{port}");

    let (out, spawned, seen) = boot_until_given_up(&bin, &locator);
    assert_gave_up_naming_the_budget(&out, &locator);
    let took = seen - spawned;
    assert!(
        took >= BUDGET - EARLY,
        "gave up {took:?} after the spawn, before its {BUDGET:?} budget was spent — a refused \
         connect is retried until the budget runs out. Output:\n{out}"
    );
    assert!(
        took <= BOOT_BEFORE_OPEN + BUDGET + SLACK,
        "gave up {took:?} after the spawn: past {BOOT_BEFORE_OPEN:?} of boot + the {BUDGET:?} \
         budget + {SLACK:?}. Output:\n{out}"
    );
}

#[test]
fn zephyr_zenoh_open_against_a_silent_listener_ends_at_its_budget() {
    let bin = image();
    // Accepts the TCP connect and never sends a byte: the zenoh handshake read
    // is the wait. The accepted streams are held so none is closed under the
    // image (a close would end the read with an error instead of a timeout).
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind the silent listener");
    let port = listener.local_addr().expect("listener address").port();
    let locator = format!("tcp/127.0.0.1:{port}");
    let accepted: Arc<Mutex<Vec<(Instant, TcpStream)>>> = Arc::default();
    {
        let accepted = Arc::clone(&accepted);
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                accepted.lock().unwrap().push((Instant::now(), stream));
            }
        });
    }

    let (out, _spawned, seen) = boot_until_given_up(&bin, &locator);
    let first_accept = accepted.lock().unwrap().first().map(|(t, _)| *t);
    let Some(first_accept) = first_accept else {
        panic!("the image never connected to the silent listener on {locator}. Output:\n{out}");
    };
    assert_gave_up_naming_the_budget(&out, &locator);
    let took = seen.saturating_duration_since(first_accept);
    assert!(
        took >= BUDGET - EARLY,
        "gave up {took:?} after the TCP connect, before its {BUDGET:?} budget was spent. \
         Output:\n{out}"
    );
    assert!(
        took <= BUDGET + SLACK,
        "gave up {took:?} after the TCP connect: the handshake read ran past the {BUDGET:?} \
         budget (+{SLACK:?}). Before issue 2389 it ran its own 10 s deadline. Output:\n{out}"
    );
}

/// RFC-0051 §7c (phase-510 W3) — the cases this file runs are exactly the
/// Runtime cells `matrix::consumer_of` assigns to it. Fixture-free.
#[test]
fn cases_bound_to_matrix_cells() {
    use nros_tests::matrix::{
        Consumer, Kind as K, Lang as L, PlatformId as P, Rmw as R, Workload as W,
        assert_consumer_cases,
    };
    assert_consumer_cases(
        Consumer::ZephyrConnectBudget,
        &[(
            P::ZephyrNativeSim,
            L::Rust,
            R::Zenoh,
            W::ConnectBudget,
            K::Example,
        )],
    );
}
