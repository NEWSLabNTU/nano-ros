//! phase-554 W4 (RFC-0128 D7, amended 2026-11-24) -- a managed node's five
//! lifecycle services share ONE zenoh queryable, through the real C shim and
//! a real router.
//!
//! Run with:
//! cargo test --features platform-posix -p nros-rmw-zenoh --test lifecycle_group

#![cfg(feature = "platform-posix")]

use nros_rmw::{
    QoSProfile, ServiceInfo, ServiceTrait, Session, SessionMode, Transport, TransportConfig,
};
use nros_rmw_zenoh::{ZenohServiceServer, ZenohTransport};
use nros_tests::fixtures::{ZenohRouter, or_unmet};
use std::time::Duration;

// ============================================================================
// phase-554 W4 — a node's five lifecycle services share ONE queryable
// ============================================================================

const LC_HASH: &str = "TypeHashNotSupported";

fn lifecycle_service(node: &'static str, endpoint: &'static str) -> ServiceInfo<'static> {
    let name: &'static str = Box::leak(format!("/w4_lc/{node}/{endpoint}").into_boxed_str());
    let ty = match endpoint {
        "change_state" => "lifecycle_msgs::srv::dds_::ChangeState_",
        "get_state" => "lifecycle_msgs::srv::dds_::GetState_",
        "get_available_states" => "lifecycle_msgs::srv::dds_::GetAvailableStates_",
        _ => "lifecycle_msgs::srv::dds_::GetAvailableTransitions_",
    };
    ServiceInfo::new(name, ty, LC_HASH)
        .with_namespace("/w4_lc")
        .with_node_name(node)
}

const LC_ENDPOINTS: [&str; 5] = [
    "change_state",
    "get_state",
    "get_available_states",
    "get_available_transitions",
    "get_transition_graph",
];

/// Through the real C shim and a real router:
///
/// 1. a node's five lifecycle servers are served by ONE queryable;
/// 2. another node gets its own;
/// 3. a sixth server for an endpoint the node already serves is REFUSED, and
///    so is a lifecycle service whose name's node is not the node it was
///    created for -- neither falls back to a queryable of its own;
/// 4. a request reaches the member its key names, and the reply comes back.
#[test]
fn a_nodes_lifecycle_services_share_one_queryable() {
    use nros_rmw::ClientTrait;

    if let Some(why) = nros_tests::process::zenohd_unavailable_reason() {
        nros_tests::unmet!("{why}");
    }
    let _router = or_unmet(ZenohRouter::start_unique());
    let router_locator = _router.locator();
    let config = TransportConfig {
        locator: Some(router_locator.as_str()),
        mode: SessionMode::Client,
        properties: &[],
        node_name: "",
        namespace: "",
        domain_id: 0,
    };
    let mut session = ZenohTransport::open(&config)
        .unwrap_or_else(|e| panic!("could not open a client session on {router_locator}: {e:?}"));
    let qos = QoSProfile::services_default();

    // 1 - one node, five servers, one queryable.
    let mut a: Vec<ZenohServiceServer> = LC_ENDPOINTS
        .iter()
        .map(|e| {
            session
                .create_service(&lifecycle_service("a", e), qos)
                .unwrap_or_else(|err| panic!("lifecycle server {e} on /w4_lc/a: {err:?}"))
        })
        .collect();
    let handle = a[0].queryable_handle();
    for (s, e) in a.iter().zip(LC_ENDPOINTS) {
        assert_eq!(
            s.queryable_handle(),
            handle,
            "{e} must share node a's queryable"
        );
    }

    // 2 - another node, its own queryable.
    let b = session
        .create_service(&lifecycle_service("b", "get_state"), qos)
        .expect("node b's first lifecycle server");
    assert_ne!(
        b.queryable_handle(),
        handle,
        "node b must not share node a's queryable"
    );

    // 3 - a sixth server for a served endpoint, and a mismatched node.
    assert!(
        session
            .create_service(&lifecycle_service("a", "get_state"), qos)
            .is_err(),
        "a second get_state on node a must be refused"
    );
    let mismatched = lifecycle_service("a", "get_state").with_node_name("someone_else");
    assert!(
        session.create_service(&mismatched, qos).is_err(),
        "a lifecycle service named under node a but created for another node must be refused"
    );

    // 4 - routing: a request on get_available_states reaches that member only.
    let target = 2;
    let mut client = session
        .create_client(&lifecycle_service("a", LC_ENDPOINTS[target]), qos)
        .expect("the client is declared");
    let request = [0x00u8, 0x01, 0x00, 0x00];
    let budget = Duration::from_secs(5);
    let deadline = std::time::Instant::now() + budget;
    let mut sent = false;
    while std::time::Instant::now() < deadline && !a[target].has_request() {
        if !sent {
            sent = client.send_request_raw(&request).is_ok();
        }
        let _ = session.spin_once(5);
    }
    assert!(
        a[target].has_request(),
        "the request never reached its member"
    );
    for (i, s) in a.iter().enumerate() {
        if i != target {
            assert!(
                !s.has_request(),
                "{} must not receive another member's request",
                LC_ENDPOINTS[i]
            );
        }
    }
    let mut buf = [0u8; 64];
    // Borrow: the first server holds the queryable, and dropping it would
    // undeclare it for every member (and release the held reply slot).
    let server = &mut a[target];
    let seq = server
        .take_request(&mut buf)
        .expect("the request is taken")
        .expect("the ring reported a request")
        .sequence_number;
    let reply = [0x00u8, 0x01, 0x00, 0x00, 0x2a, 0, 0, 0];
    server
        .send_response(seq, &reply)
        .expect("the reply is sent on the query's own key");
    let mut got = [0u8; 64];
    let deadline = std::time::Instant::now() + budget;
    let verdict = loop {
        let _ = session.spin_once(5);
        match client.take_response_raw(&mut got) {
            Ok(None) if std::time::Instant::now() < deadline => continue,
            other => break other,
        }
    };
    assert!(
        matches!(verdict, Ok(Some(_))),
        "the reply never came back: {verdict:?}"
    );

    drop(client);
    drop(a);
    drop(b);
    session.close().expect("Failed to close session");
}

/// phase-554 W4 -- the queryable table is sized from a DERIVATION
/// (`nros_sizing_descriptor::infra::infra_zenoh_queryables_with`: one
/// queryable per managed node) while the request headers are sized from the
/// SERVER count (five per node). This holds the derivation to what the shim
/// actually declares at run time, both ways: two managed nodes register ten
/// lifecycle servers and the session declares exactly the derived number of
/// queryables -- two, not ten -- and the attached servers are the difference.
#[test]
fn the_queryables_the_shim_declares_are_the_ones_the_build_derives() {
    use nros_sizing_descriptor::infra::{
        infra_service_servers_with, infra_zenoh_queryables_with, lifecycle_attached_servers,
    };

    if let Some(why) = nros_tests::process::zenohd_unavailable_reason() {
        nros_tests::unmet!("{why}");
    }
    let _router = or_unmet(ZenohRouter::start_unique());
    let router_locator = _router.locator();
    let config = TransportConfig {
        locator: Some(router_locator.as_str()),
        mode: SessionMode::Client,
        properties: &[],
        node_name: "",
        namespace: "",
        domain_id: 0,
    };
    let mut session = ZenohTransport::open(&config)
        .unwrap_or_else(|e| panic!("could not open a client session on {router_locator}: {e:?}"));
    let qos = QoSProfile::services_default();
    let before = session
        .inner()
        .queryables_declared()
        .expect("the shim counts its queryables");

    let mut servers = Vec::new();
    for node in ["n1", "n2"] {
        for e in LC_ENDPOINTS {
            servers.push(
                session
                    .create_service(&lifecycle_service(node, e), qos)
                    .unwrap_or_else(|err| panic!("lifecycle server {e} on {node}: {err:?}")),
            );
        }
    }
    let declared = session
        .inner()
        .queryables_declared()
        .expect("the shim counts its queryables")
        - before;

    // The image these servers make: the lifecycle family alone, two machines.
    let (infra, machines) = (Some("lifecycle"), Some("2"));
    let derived = infra_zenoh_queryables_with(infra, None, machines);
    assert_eq!(
        declared as usize, derived,
        "the shim declared {declared} queryable(s) for two managed nodes; the build derives {derived}"
    );
    assert_eq!(
        servers.len(),
        infra_service_servers_with(infra, None, machines),
        "every lifecycle service is still a server with its own header and ring"
    );
    assert_eq!(
        servers.len() - declared as usize,
        lifecycle_attached_servers(infra, machines),
        "the servers without a queryable of their own are the attached ones"
    );

    drop(servers);
    assert_eq!(
        session.inner().queryables_declared(),
        Some(before),
        "dropping the servers undeclares the nodes' queryables"
    );
    session.close().expect("Failed to close session");
}
