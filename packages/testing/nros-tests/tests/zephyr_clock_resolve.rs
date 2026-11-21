//! A Zephyr image resolves its wall clock after registration and before the
//! first spin, and the wait is owed to no timer (phase-582 W1c-2, RFC-0141 D1).
//!
//! **Bucket (phase-329 W4): matrix consumer.** The configured cells
//! `(ZephyrNativeSim, Rust, Zenoh, Pubsub, Example) + [ClockHlc]` and
//! `+ [ClockSntp]` of `matrix::CELLS`.
//!
//! The images are the Rust workspace pubsub bringup's
//! `zephyr_listener_clock_hlc` (`source = "zenoh-hlc,platform"`, a 4 s settle)
//! and `zephyr_talker_clock_sntp` (`source = "sntp,platform"`, two 1.5 s
//! attempts at a server that never answers). Each case boots its image with
//! its own router (`-testargs --nros-locator=`). The resolver prints its
//! verdict (`nros: wall clock from <source>`) and what the boot paid for it on
//! the image's own clock (`nros: clock resolution took <n> ms`), and the cases
//! bound both:
//!
//! - a native talker publishing through the router: the listener latches the
//!   HLC (the router stamps the unstamped sample), after the talker started
//!   and before the settle time ran out;
//! - no publisher: the listener falls back to the platform, and not before
//!   the settle time was spent;
//! - the SNTP talker: nothing publishes before the resolution, its first
//!   publish comes a timer period after it rather than at once, and the
//!   periods after that keep pace: the wait is not replayed as a burst.
//!
//! ## Prerequisites
//!
//! - `just zephyr build-fixtures` (the west lane builds
//!   `build-ws-rust-listener-clock-hlc-zenoh` and
//!   `build-ws-rust-talker-clock-sntp-zenoh`), and the native talker fixture
//! - `rmw_zenohd` (the router)
//!
//! Run with: `cargo nextest run -p nros-tests --test zephyr_clock_resolve`

use nros_tests::{
    fixtures::{RequireFixture, ZenohRouter, build_native_talker, workspace_image::PairRole},
    matrix::{Config, Lang, PlatformId, Rmw},
    output::{
        CLOCK_FROM_PREFIX, CLOCK_RESOLUTION_TOOK_PREFIX, LISTENER_LOG_PREFIX,
        LISTENER_READY_MARKER, TALKER_LOG_PREFIX, clock_latched_source, clock_resolution_ms,
    },
    process::ManagedProcess,
    zephyr::{ZephyrPlatform, ZephyrProcess},
};
use std::{
    path::PathBuf,
    process::Command,
    time::{Duration, Instant},
};

/// `[image.zephyr_listener_clock_hlc.knobs.clock] fallback_settle_ms`.
const SETTLE_MS: u64 = 4000;

/// What a resolution may overrun its settle time by: the source's 10 ms poll,
/// the console, and a loaded native_sim host. A replay or a second settle
/// would be thousands of ms over.
const SETTLE_SLACK_MS: u64 = 1500;

/// How long the HLC case waits after the listener's subscription exists before
/// it starts the talker. The HLC can only latch after that, so a resolution
/// shorter than this latched from something else.
const TALKER_DELAY: Duration = Duration::from_millis(1500);

/// `[image.zephyr_talker_clock_sntp]`: `sntp_attempts` x `sntp.timeout_ms`.
const SNTP_WAIT_MS: u64 = 2 * 1500;

/// The talker's publish period (`talker_pkg`, `TimerDuration::from_millis(1000)`).
const PERIOD: Duration = Duration::from_millis(1000);

/// Long enough for every case's boot plus its settle time to show as a timing
/// failure rather than a missing line.
const WAIT: Duration = Duration::from_secs(40);

fn image(config: Config, role: PairRole) -> PathBuf {
    nros_tests::fixtures::lane::require_platform_in_lane(
        &[PlatformId::ZephyrNativeSim],
        "the Zephyr wall-clock cells",
    );
    nros_tests::fixtures::workspace_image::resolve_configured(
        PlatformId::ZephyrNativeSim,
        Lang::Rust,
        Rmw::Zenoh,
        role,
        &[config],
    )
    .unwrap_or_else(|e| {
        nros_tests::unmet!(
            "zephyr rust {} image for {} not prebuilt or stale \
             (run `just zephyr build-fixtures`): {e:?}",
            role.token(),
            config.as_str()
        )
    })
}

/// Everything the image has printed so far, without waiting.
fn snapshot(z: &ZephyrProcess) -> String {
    z.wait_for_pattern(CLOCK_RESOLUTION_TOOK_PREFIX, Duration::ZERO)
}

/// The resolution's verdict and duration, or a failure naming what is missing.
fn resolution(out: &str) -> (Option<String>, u64) {
    let Some(took) = clock_resolution_ms(out) else {
        panic!(
            "the image never resolved its clock (`{CLOCK_RESOLUTION_TOOK_PREFIX}`). Output:\n{out}"
        );
    };
    (clock_latched_source(out).map(str::to_owned), took)
}

/// The output after the resolution line: what the image did once the gate
/// opened.
fn after_resolution(out: &str) -> &str {
    out.find(CLOCK_RESOLUTION_TOOK_PREFIX)
        .map_or("", |i| &out[i..])
}

#[test]
fn zephyr_clock_latches_the_hlc_of_a_sample_before_the_first_spin() {
    let bin = image(Config::ClockHlc, PairRole::Listener);
    let talker_bin = build_native_talker().require("native-rs-talker");
    let router = nros_tests::fixtures::or_unmet(ZenohRouter::start_unique());
    let locator = router.locator();

    let zephyr = ZephyrProcess::start_with_locator(&bin, ZephyrPlatform::NativeSim, &locator)
        .expect("failed to start the native_sim listener");
    let ready = zephyr.wait_for_pattern(LISTENER_READY_MARKER, WAIT);
    assert!(
        ready.contains(LISTENER_READY_MARKER),
        "the listener never created its subscription (`{LISTENER_READY_MARKER}`). Output:\n{ready}"
    );
    std::thread::sleep(TALKER_DELAY);

    let mut cmd = Command::new(talker_bin);
    cmd.env("NROS_LOCATOR", &locator).env("RUST_LOG", "info");
    let talker = ManagedProcess::spawn_image(cmd, "native-rs-talker").expect("start the talker");

    let out = zephyr.wait_for_pattern(CLOCK_RESOLUTION_TOOK_PREFIX, WAIT);
    let (source, took) = resolution(&out);
    assert_eq!(
        source.as_deref(),
        Some("zenoh-hlc"),
        "with a talker publishing through the router the listener must latch the HLC \
         (`{CLOCK_FROM_PREFIX}zenoh-hlc`). Output:\n{out}"
    );
    assert!(
        took >= TALKER_DELAY.as_millis() as u64,
        "latched {took} ms into the resolution, before the talker started \
         ({TALKER_DELAY:?} after the subscription): the HLC came from something else. \
         Output:\n{out}"
    );
    assert!(
        took < SETTLE_MS,
        "latched {took} ms into the resolution, at or past the {SETTLE_MS} ms settle time: \
         the sample did not reach the waiting source. Output:\n{out}"
    );

    // The gate opened, so the listener spins and dispatches.
    let heard = zephyr.wait_for_pattern(LISTENER_LOG_PREFIX, Duration::from_secs(10));
    assert!(
        after_resolution(&heard).contains(LISTENER_LOG_PREFIX),
        "the listener resolved its clock and then dispatched nothing (`{LISTENER_LOG_PREFIX}`). \
         Output:\n{heard}"
    );
    drop(talker);
    drop(router);
}

#[test]
fn zephyr_clock_falls_back_after_the_settle_time_when_no_sample_comes() {
    let bin = image(Config::ClockHlc, PairRole::Listener);
    let router = nros_tests::fixtures::or_unmet(ZenohRouter::start_unique());

    let zephyr =
        ZephyrProcess::start_with_locator(&bin, ZephyrPlatform::NativeSim, &router.locator())
            .expect("failed to start the native_sim listener");
    let out = zephyr.wait_for_pattern(CLOCK_RESOLUTION_TOOK_PREFIX, WAIT);
    let (source, took) = resolution(&out);
    assert_eq!(
        source.as_deref(),
        Some("platform"),
        "with no publisher the listener must fall back to the platform \
         (`{CLOCK_FROM_PREFIX}platform`). Output:\n{out}"
    );
    assert!(
        took >= SETTLE_MS,
        "fell back {took} ms into the resolution, before the {SETTLE_MS} ms settle time was \
         spent. Output:\n{out}"
    );
    assert!(
        took <= SETTLE_MS + SETTLE_SLACK_MS,
        "fell back {took} ms into the resolution: past the {SETTLE_MS} ms settle time + \
         {SETTLE_SLACK_MS} ms. Output:\n{out}"
    );
    drop(router);
}

#[test]
fn zephyr_clock_wait_is_not_replayed_by_the_timers() {
    let bin = image(Config::ClockSntp, PairRole::Talker);
    let router = nros_tests::fixtures::or_unmet(ZenohRouter::start_unique());

    let zephyr =
        ZephyrProcess::start_with_locator(&bin, ZephyrPlatform::NativeSim, &router.locator())
            .expect("failed to start the native_sim talker");
    let out = zephyr.wait_for_pattern(CLOCK_RESOLUTION_TOOK_PREFIX, WAIT);
    let resolved_at = Instant::now();
    let (source, took) = resolution(&out);
    assert_eq!(
        source.as_deref(),
        Some("platform"),
        "an SNTP server that never answers must fall back to the platform. Output:\n{out}"
    );
    // The case means something only if boot really waited. A host with no
    // route answers the SNTP send at once (ENETUNREACH) and spends nothing.
    if took < SNTP_WAIT_MS / 2 {
        nros_tests::unmet!(
            "the SNTP attempts took {took} ms, not the ~{SNTP_WAIT_MS} ms of two timeouts: this \
             host answers a send to 192.0.2.1 at once, so there is no wait to replay. Output:\n{out}"
        );
    }
    assert!(
        took <= SNTP_WAIT_MS + SETTLE_SLACK_MS,
        "the SNTP attempts took {took} ms: past {SNTP_WAIT_MS} ms + {SETTLE_SLACK_MS} ms. \
         Output:\n{out}"
    );
    let before = &out[..out.find(CLOCK_RESOLUTION_TOOK_PREFIX).unwrap_or(out.len())];
    assert!(
        !before.contains(TALKER_LOG_PREFIX),
        "the talker published before its clock resolved: nothing may stamp before the gate \
         opens. Output:\n{out}"
    );

    // The first publish: one period after the resolution, alone. A timer that
    // still owed the wait would fire at once, several times.
    let (first_at, first) = loop {
        let now = snapshot(&zephyr);
        let n = after_resolution(&now).matches(TALKER_LOG_PREFIX).count();
        if n > 0 {
            break (Instant::now(), now);
        }
        assert!(
            resolved_at.elapsed() < 3 * PERIOD,
            "no publish within {:?} of the resolution. Output:\n{now}",
            3 * PERIOD
        );
        std::thread::sleep(Duration::from_millis(20));
    };
    let gap = first_at - resolved_at;
    let burst = after_resolution(&first).matches(TALKER_LOG_PREFIX).count();
    assert!(
        gap >= PERIOD * 6 / 10,
        "the first publish came {gap:?} after the resolution, well under the {PERIOD:?} period: \
         the timer replayed the wait. Output:\n{first}"
    );
    assert!(
        gap <= PERIOD * 25 / 10,
        "the first publish came {gap:?} after the resolution: past 2.5 periods. Output:\n{first}"
    );
    assert_eq!(
        burst, 1,
        "the first spin published {burst} times at once: the timer replayed the wait. \
         Output:\n{first}"
    );

    // And the periods after keep pace: three periods, three publishes, give or
    // take one for where the window cuts.
    std::thread::sleep(3 * PERIOD);
    let later = snapshot(&zephyr);
    let more = after_resolution(&later).matches(TALKER_LOG_PREFIX).count() - burst;
    assert!(
        (2..=4).contains(&more),
        "{more} publishes in the {:?} after the first, against one per {PERIOD:?}. \
         Output:\n{later}",
        3 * PERIOD
    );
    drop(router);
}

/// RFC-0051 §7a/§7c — this file answers the configured cells
/// `(ZephyrNativeSim, Rust, Zenoh, Pubsub, Example) + [ClockHlc]` (the latch
/// and the fallback) and `+ [ClockSntp]` (the timers). Fixture-free.
#[test]
fn cases_bound_to_matrix_cells() {
    use nros_tests::matrix::{
        Consumer, Kind as K, Lang as L, PlatformId as P, Rmw as R, Workload as W,
        assert_consumer_config_cases,
    };
    const PAIR: (P, L, R, W, K) = (P::ZephyrNativeSim, L::Rust, R::Zenoh, W::Pubsub, K::Example);
    assert_consumer_config_cases(
        Consumer::ZephyrClockResolve,
        &[(PAIR, &[Config::ClockHlc]), (PAIR, &[Config::ClockSntp])],
    );
}
