//! Phase 115.F — Multi-process custom-transport loopback test.
//!
//! Validates the full Phase 115 pipeline end-to-end:
//!
//! ```text
//!     talker (process A)             zenohd                  listener (process B)
//!     ──────────────────             ──────                  ────────────────────
//!     Publisher<Int32>                                       Subscription<Int32>
//!         │                                                          ▲
//!         ▼ zenoh wire bytes                       zenoh wire bytes  │
//!     custom://callbacks ──tcp──▶  127.0.0.1:N  ──tcp──▶  custom://callbacks
//! ```
//!
//! Both nano-ros processes register a user-supplied transport
//! whose `read`/`write` callbacks proxy to a regular `TcpStream`.
//! That stream connects to a real `zenohd` running in a third
//! process. Pub/sub round-trips through three TCP hops + two
//! user-vtable invocations per message.
//!
//! Two processes are required because zpico-sys keeps
//! per-session state in `static` arrays — only one zenoh-pico
//! session can live in a single process at a time.
//!
//! Run via:
//! ```bash
//! cargo test -p nros-tests --features rmw --test custom_transport_loopback
//! ```

use nros_tests::{
    count_pattern,
    fixtures::{
        ManagedProcess, ZenohRouter, build_native_custom_transport_listener,
        build_native_custom_transport_talker, require_zenohd, zenohd_unique,
    },
};
use rstest::rstest;
use std::{process::Command, time::Duration};

#[rstest]
fn test_custom_transport_loopback(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    let talker_bin = build_native_custom_transport_talker().expect("build talker");
    let listener_bin = build_native_custom_transport_listener().expect("build listener");
    let locator = zenohd_unique.locator();

    // Both binaries treat the locator as a TCP target — strip the
    // `tcp/` scheme prefix to get a `host:port` pair.
    let tcp_target = locator
        .strip_prefix("tcp/")
        .expect("zenohd locator is tcp/...")
        .to_string();

    // Listener first so it's subscribed before the talker starts emitting
    // (zenoh-pico is volatile-by-default). The listener stays up long
    // enough for the readiness + count waits below to drive the test;
    // the waits, not the lifetime, bound how long this test runs.
    let mut listener_cmd = Command::new(listener_bin);
    listener_cmd
        .env("RUST_LOG", "info")
        .env("NROS_CUSTOM_TCP_TARGET", &tcp_target)
        .env("NROS_LISTENER_SECS", "20");
    let mut listener =
        ManagedProcess::spawn_command(listener_cmd, "listener").expect("spawn listener");

    // Wait for the subscription declaration to complete instead of a fixed
    // sleep — "Subscriber created on /chatter" is the listener's readiness
    // marker (Phase 179.G; the custom-link declaration path was unblocked
    // once the full-duplex TcpStream deadlock was fixed).
    listener
        .wait_for_output_pattern("Subscriber created", Duration::from_secs(15))
        .expect("listener did not declare its subscription");

    let mut talker_cmd = Command::new(talker_bin);
    talker_cmd
        .env("RUST_LOG", "info")
        .env("NROS_CUSTOM_TCP_TARGET", &tcp_target)
        .env("NROS_TALKER_COUNT", "20");
    let mut talker = ManagedProcess::spawn_command(talker_cmd, "talker").expect("spawn talker");

    // Talker must publish at least one message.
    let talker_out = talker
        .wait_for_output_count(
            nros_tests::output::TALKER_LOG_PREFIX,
            1,
            Duration::from_secs(15),
        )
        .expect("talker did not publish any message");

    // Listener must receive messages through the custom-transport loopback.
    // Waiting for three (rather than one) confirms the data plane keeps
    // flowing after pub/sub matching, not just a single lucky frame.
    let listener_out = listener
        .wait_for_output_count(
            nros_tests::output::LISTENER_LOG_PREFIX,
            3,
            Duration::from_secs(20),
        )
        .expect("listener did not receive messages via custom-transport loopback");

    talker.kill();
    listener.kill();

    println!("=== Talker output ===\n{talker_out}");
    println!("=== Listener output ===\n{listener_out}");

    let published = count_pattern(&talker_out, nros_tests::output::TALKER_LOG_PREFIX);
    let received = count_pattern(&listener_out, nros_tests::output::LISTENER_LOG_PREFIX);
    println!("Published: {published}, Received: {received}");

    assert!(published > 0, "talker must publish at least one message");
    assert!(
        received >= 3,
        "listener must receive multiple messages via custom-transport loopback"
    );
}
