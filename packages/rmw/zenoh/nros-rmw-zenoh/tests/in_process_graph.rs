//! Issue 2083 — a client sees a service server its OWN session hosts.
//!
//! Needs `rmw_zenohd` (an absent router is an unmet precondition, not a pass).
//! Runs in `just check required-features-tests` (`.config/feature-lanes.toml`).

#![cfg(feature = "platform-posix")]

use nros_rmw::{ClientTrait, QoSProfile, ServiceInfo, SessionMode, Transport, TransportConfig};
use nros_rmw_zenoh::ZenohTransport;
use nros_tests::fixtures::{ZenohRouter, or_unmet};
use std::{thread, time::Duration};

fn router() -> ZenohRouter {
    if let Some(why) = nros_tests::process::zenohd_unavailable_reason() {
        nros_tests::unmet!("{why}");
    }
    or_unmet(ZenohRouter::start_unique())
}

// ============================================================================
// issue 2083 — a client sees a server its own session hosts
// ============================================================================

/// The service both halves name; its own key, so nothing else answers on it.
const IN_PROCESS_SERVICE: ServiceInfo<'static> = ServiceInfo::new(
    "/nros_in_process_service",
    "example_interfaces/srv/AddTwoInts",
    "TypeHashNotSupported",
)
// A node name is what makes the shim declare the entity's liveliness token,
// as every rclcpp/rclrs entity has one.
.with_node_name("in_process_service");

/// Poll `ready` until it holds, or give up after `secs`.
fn wait_until(secs: u64, mut ready: impl FnMut() -> bool) -> bool {
    let deadline = std::time::Instant::now() + Duration::from_secs(secs);
    loop {
        if ready() {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        thread::sleep(Duration::from_millis(50));
    }
}

/// issue 2083 — zenoh-pico never delivers a session's own liveliness tokens
/// to that session's own liveliness subscriber, so the graph cache
/// `service_is_ready` reads held every server but the ones this process
/// hosts. ros2/demos `composition` (a client and its server in one process)
/// waited forever on posix while `ros2 service call` was answered.
///
/// One session: the client exists BEFORE the server (the cache must take a
/// token declared after it started) and the server goes away again (its token
/// must leave). Measured: on the unfixed shim the first wait times out.
#[test]
fn a_client_sees_a_server_its_own_session_hosts() {
    use nros_rmw::Session as _;
    let router = router();
    let locator = router.locator();
    let config = TransportConfig {
        locator: Some(locator.as_str()),
        mode: SessionMode::Client,
        properties: &[("multicast_scouting", "false")],
        node_name: "in_process_service",
        namespace: "",
        domain_id: 0,
    };
    let mut session = ZenohTransport::open(&config).expect("open zenoh session");
    let qos = QoSProfile::services_default();
    let client = session
        .create_client(&IN_PROCESS_SERVICE, qos)
        .expect("declare the client");
    assert!(
        !matches!(client.service_is_ready(), Ok(true)),
        "no server exists yet, and service_is_ready said it does"
    );

    let server = session
        .create_service(&IN_PROCESS_SERVICE, qos)
        .expect("declare the server");
    assert!(
        wait_until(5, || matches!(client.service_is_ready(), Ok(true))),
        "a server declared on the client's own session never became visible to it (issue 2083)"
    );

    drop(server);
    assert!(
        wait_until(5, || matches!(client.service_is_ready(), Ok(false))),
        "the server was dropped, and its own session still listed it"
    );
    drop(client);
    session.close().expect("close the session");
}
