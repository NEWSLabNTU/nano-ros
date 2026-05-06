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

    // Listener first so it's subscribed before the talker starts
    // emitting (zenoh-pico is volatile-by-default).
    let mut listener_cmd = Command::new(listener_bin);
    listener_cmd
        .env("RUST_LOG", "info")
        .env("NROS_CUSTOM_TCP_TARGET", &tcp_target)
        .env("NROS_LISTENER_SECS", "8");
    let mut listener =
        ManagedProcess::spawn_command(listener_cmd, "listener").expect("spawn listener");

    std::thread::sleep(Duration::from_secs(2));

    let mut talker_cmd = Command::new(talker_bin);
    talker_cmd
        .env("RUST_LOG", "info")
        .env("NROS_CUSTOM_TCP_TARGET", &tcp_target)
        .env("NROS_TALKER_COUNT", "20");
    let mut talker = ManagedProcess::spawn_command(talker_cmd, "talker").expect("spawn talker");

    // Let them communicate.
    std::thread::sleep(Duration::from_secs(5));

    talker.kill();
    listener.kill();

    let talker_out = talker.wait_for_all_output(Duration::from_secs(2)).unwrap_or_default();
    let listener_out = listener.wait_for_all_output(Duration::from_secs(2)).unwrap_or_default();

    println!("=== Talker output ===\n{talker_out}");
    println!("=== Listener output ===\n{listener_out}");

    let published = count_pattern(&talker_out, "Published:");
    let received = count_pattern(&listener_out, "Received:");

    println!("Published: {published}, Received: {received}");

    assert!(published > 0, "talker must publish at least one message");
    assert!(
        received > 0,
        "listener must receive at least one message via custom-transport loopback"
    );
}
