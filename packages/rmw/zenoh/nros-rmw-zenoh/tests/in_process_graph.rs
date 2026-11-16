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

// ============================================================================
// issue 2493 -- every node is a publisher of one shared data key
// ============================================================================

/// issue 2493 -- two nodes' writers on one topic are two publishers in the
/// graph (a liveliness token and a gid each) over ONE zenoh publisher, and
/// the data key outlives whichever holder drops first.
///
/// One `declare_node_topic_writer` per node plus an ordinary
/// `create_publisher` on the same key: the graph counts three publishers,
/// all three share one zenoh handle, the gids differ, and after the FIRST
/// declarer drops the others still put.
#[test]
fn node_topic_writers_share_one_data_key_and_each_is_a_publisher() {
    use nros_rmw::{Publisher as _, Session as _, TopicInfo};
    let router = router();
    let locator = router.locator();
    let config = TransportConfig {
        locator: Some(locator.as_str()),
        mode: SessionMode::Client,
        properties: &[("multicast_scouting", "false")],
        node_name: "node_topic_writers",
        namespace: "",
        domain_id: 0,
    };
    let mut session = ZenohTransport::open(&config).expect("open zenoh session");
    const TOPIC: &str = "/nros_node_topic_writer_events";
    let topic = |node| {
        TopicInfo::new(
            TOPIC,
            "rcl_interfaces::msg::dds_::ParameterEvent_",
            "TypeHashNotSupported",
        )
        .with_node_name(node)
    };
    let qos = QoSProfile::parameter_events_default();

    let mut alpha = session
        .declare_node_topic_writer(&topic("alpha"), &qos)
        .expect("alpha's writer");
    let mut beta = session
        .declare_node_topic_writer(&topic("beta"), &qos)
        .expect("beta's writer");
    let gamma = session
        .create_publisher(&topic("gamma"), qos)
        .expect("gamma's ordinary publisher");
    assert_eq!(
        alpha.zenoh_handle(),
        beta.zenoh_handle(),
        "one zenoh publisher for the data key"
    );
    assert_ne!(alpha.gid(), beta.gid(), "each writer has its own gid");
    assert!(alpha.has_liveliness() && beta.has_liveliness());
    assert!(
        wait_until(5, || matches!(session.count_publishers(TOPIC), Ok(3))),
        "the graph lists every writer as a publisher: {:?}",
        session.count_publishers(TOPIC)
    );

    alpha.put(&session, &[0, 1, 0, 0]).expect("alpha puts");
    // The first declarer goes; the data key stays up for the others.
    drop(alpha);
    assert!(
        wait_until(5, || matches!(session.count_publishers(TOPIC), Ok(2))),
        "alpha's token left with it: {:?}",
        session.count_publishers(TOPIC)
    );
    beta.put(&session, &[0, 1, 0, 0]).expect("beta still puts");
    gamma
        .publish_raw(&[0, 1, 0, 0])
        .expect("gamma still publishes");

    // VOLATILE only, and a writer is a node's.
    let mut latched = qos;
    latched.durability = nros_rmw::QoSDurabilityPolicy::TransientLocal;
    assert!(matches!(
        session.declare_node_topic_writer(&topic("delta"), &latched),
        Err(nros_rmw::TransportError::Backend(_))
    ));
    let anonymous = TopicInfo::new(
        TOPIC,
        "rcl_interfaces::msg::dds_::ParameterEvent_",
        "TypeHashNotSupported",
    );
    assert!(matches!(
        session.declare_node_topic_writer(&anonymous, &qos),
        Err(nros_rmw::TransportError::InvalidArgument)
    ));
    // `create_publisher` shares only the runtime's stock writers: gamma is a
    // user topic, so it has a zenoh publisher of its own.
    assert_ne!(
        gamma.zenoh_handle(),
        beta.zenoh_handle(),
        "a user publisher never shares implicitly"
    );
    drop(beta);
    drop(gamma);
    session.close().expect("close the session");
}

/// Issue 2493 -- through the ordinary publisher path, two nodes'
/// `/parameter_events` writers share one zenoh publisher, while two user
/// publishers on one key do NOT: each keeps its own, as before the change, so
/// their events, deadlines and lending stay what they were.
#[test]
fn only_the_stock_writers_share_a_zenoh_publisher() {
    use nros_rmw::{Session as _, TopicInfo};
    let router = router();
    let locator = router.locator();
    let config = TransportConfig {
        locator: Some(locator.as_str()),
        mode: SessionMode::Client,
        properties: &[("multicast_scouting", "false")],
        node_name: "stock_writer_sharing",
        namespace: "",
        domain_id: 0,
    };
    let mut session = ZenohTransport::open(&config).expect("open zenoh session");
    let qos = QoSProfile::parameter_events_default();
    let on = |name, node| {
        TopicInfo::new(
            name,
            "rcl_interfaces::msg::dds_::ParameterEvent_",
            "TypeHashNotSupported",
        )
        .with_node_name(node)
    };

    let alpha = session
        .create_publisher(&on("/parameter_events", "alpha"), qos)
        .expect("alpha's /parameter_events writer");
    let beta = session
        .create_publisher(&on("/parameter_events", "beta"), qos)
        .expect("beta's /parameter_events writer");
    assert_eq!(
        alpha.zenoh_handle(),
        beta.zenoh_handle(),
        "the stock writers of two nodes share one zenoh publisher"
    );

    const USER: &str = "/nros_user_topic_two_publishers";
    let first = session
        .create_publisher(&on(USER, "alpha"), qos)
        .expect("first user publisher");
    let second = session
        .create_publisher(&on(USER, "beta"), qos)
        .expect("second user publisher");
    assert_ne!(
        first.zenoh_handle(),
        second.zenoh_handle(),
        "two user publishers on one key keep a zenoh publisher each"
    );
    drop((alpha, beta, first, second));
    session.close().expect("close the session");
}
