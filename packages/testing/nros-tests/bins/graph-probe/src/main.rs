//! phase-381 acceptance fixture — print the ROS graph as this node SEES it.
//!
//! Every other check in phase-381 is against our own builders, our own parser
//! and our own vtable: they prove the pieces agree with each other, not that a
//! real ROS 2 node is discovered. This binary is the half that can only be
//! answered live — it opens a session, polls until the view settles, and prints
//! what it found so the test can compare it against `ros2 node list`.
//!
//! **Polls, does not sample once.** The graph slots report what has already
//! arrived and never block, so a single call after startup legitimately returns
//! a partial graph. That is the documented behaviour, and a test written as one
//! comparison would be flaky by construction — Design note 3 of the phase doc.
//! So: poll until two consecutive sweeps agree and at least one node is seen, or
//! the budget expires.
//!
//! Environment:
//!   `GRAPH_PROBE_TIMEOUT_MS`  total budget (default 15000)
//!   `GRAPH_PROBE_EXPECT_NODE` if set, exit non-zero unless a node whose name
//!                             contains this substring is seen. That makes the
//!                             absence of a peer a FAILURE rather than a quiet
//!                             empty print — an empty graph is what this test is
//!                             most likely to get wrong.

use std::time::{Duration, Instant};

use nros::{Executor, ExecutorConfig};

fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn main() {
    let _ = env_logger::try_init();

    let locator = std::env::var("NROS_LOCATOR").unwrap_or_else(|_| "tcp/127.0.0.1:7447".into());
    let budget_ms = env_u64("GRAPH_PROBE_TIMEOUT_MS", 15_000);
    let expect_node = std::env::var("GRAPH_PROBE_EXPECT_NODE").ok();

    // Register whichever backend the `rmw-*` feature linked, through the same
    // seam the examples and `int32-sink` use. This was a hardcoded
    // `nros_rmw_zenoh::register()`, which pinned the fixture to one RMW — the
    // exact thing that made `int32-sink` unusable as the XRCE half of a bridge
    // test (phase-338 W3). A graph probe that can only speak zenoh cannot
    // answer whether CYCLONE reads the graph, which is the next question.
    nros_board_linux::register_linked_rmw();
    let config = ExecutorConfig::new(&locator).node_name("graph_probe");
    let mut executor = Executor::open(&config).expect("open session");
    let _node = executor.create_node("graph_probe").expect("create node");

    println!("GRAPH_PROBE_READY locator={locator}");

    let deadline = Instant::now() + Duration::from_millis(budget_ms);
    let mut previous: Vec<String> = Vec::new();
    let mut settled: Vec<String> = Vec::new();

    while Instant::now() < deadline {
        // Drive I/O first: the backend fills its view from the spin loop, so a
        // probe that never spins never discovers anything.
        executor.spin_once(Duration::from_millis(200));

        let mut nodes: Vec<String> = Vec::new();
        let r = executor.get_node_names(&mut |name, ns, _enclave| {
            nodes.push(format!("{ns}|{name}"));
            true
        });
        match r {
            Ok(()) => {}
            Err(e) => {
                // UNSUPPORTED is a legitimate answer from a backend with no
                // graph, and it is NOT the same as an empty one — say which.
                println!("GRAPH_PROBE_UNSUPPORTED err={e:?}");
                std::process::exit(3);
            }
        }
        nodes.sort();
        nodes.dedup();

        // Convergence is "stable AND contains what we are waiting for", not
        // merely "stable and non-empty".
        //
        // Our OWN node is in the graph from the first poll, so a non-empty
        // stable list is reached almost immediately and says nothing about
        // whether a peer was discovered. Measured on Cyclone: the probe settled
        // on `["/|graph_probe"]` after 4.1 s and reported the talker missing,
        // against a talker that was up — a false, specific claim produced by
        // the probe rather than by the slot, which is the failure this whole
        // family is about. zenoh happened to hide it by discovering the peer
        // before the loop could settle.
        let have_expected = expect_node
            .as_ref()
            .map(|want| nodes.iter().any(|n| n.contains(want)))
            .unwrap_or(true);
        if !nodes.is_empty() && nodes == previous && have_expected {
            settled = nodes;
            break;
        }
        previous = nodes;
    }

    // Budget exhausted without convergence: report what the LAST poll saw
    // rather than an empty list, so the failure names what WAS visible.
    if settled.is_empty() {
        settled = previous.clone();
    }
    for n in &settled {
        println!("GRAPH_NODE {n}");
    }
    println!("GRAPH_PROBE_NODE_COUNT {}", settled.len());

    // Topics too — the second acceptance question, and it must be POLLED for
    // the same reason nodes are.
    //
    // Nodes and entities are separate standing queries (a node token is 9
    // chunks, an entity token 13, and one wildcard cannot match both shapes),
    // so warming up the node view says nothing about the entity view. Calling
    // this once after the node loop reported ZERO topics against a live talker
    // that had several — a defect in this probe, not in the slot, and exactly
    // the shape the API docs warn about: one call is not an answer.
    let mut topics: Vec<String> = Vec::new();
    let topic_deadline = Instant::now() + Duration::from_millis(budget_ms);
    let mut prev_topics: Vec<String> = Vec::new();
    while Instant::now() < topic_deadline {
        executor.spin_once(Duration::from_millis(200));
        let mut found: Vec<String> = Vec::new();
        // NOT `let _ =`. Swallowing this is what hid issue 0903: the call was
        // returning `Unsupported` — the runtime dispatched only
        // `get_node_names` and let every other graph method fall through to the
        // trait default — and a discarded error read as "no topics on this
        // graph". An unreported error is indistinguishable from an empty
        // answer, which is the exact distinction this whole family is about.
        if let Err(e) = executor.get_topic_names_and_types(&mut |name, types| {
            found.push(format!("{name} [{}]", types.join(",")));
            true
        }) {
            println!("GRAPH_PROBE_TOPICS_ERR {e:?}");
            break;
        }
        found.sort();
        found.dedup();
        if !found.is_empty() && found == prev_topics {
            topics = found;
            break;
        }
        prev_topics = found;
    }
    topics.sort();
    for t in &topics {
        println!("GRAPH_TOPIC {t}");
    }
    println!("GRAPH_PROBE_TOPIC_COUNT {}", topics.len());

    // ---------------------------------------------------------------------
    // The other NINE slots — phase-381 W4/W5, and the reason this probe grew.
    //
    // Two slots were proven against a live peer (issue 0903) and nine were not.
    // `check-rmw-slot-producers` calls all eleven `produced`, but that means
    // something writes the slot and something reads it — NOT that either ever
    // met a real ROS 2 node. Phase-393's closing note is exactly this trap, and
    // phase-381 is where it bit: twelve slots produced, mutation-tested and
    // parity-clean, and the feature did not work at all.
    //
    // So each call below is CHECKED, not printed. An `Err` fails the probe:
    // `Unsupported` from a runtime that claims the slot is the 0903 defect
    // recurring, and it is indistinguishable from an empty graph unless
    // something says so.
    // The node these checks are ABOUT, and a topic it publishes. Both come
    // from the environment so the probe is not welded to `demo_nodes_cpp`;
    // the defaults are what the interop cell starts.
    let node = expect_node
        .clone()
        .unwrap_or_else(|| "talker".to_string());
    let topic = std::env::var("GRAPH_PROBE_TOPIC").unwrap_or_else(|_| "/chatter".to_string());
    let topic = topic.as_str();

    let mut failures: Vec<String> = Vec::new();
    // A backend may legitimately answer FEWER slots than zenoh — Cyclone's W5
    // reader serves `get_node_names` and nothing else — and W6's whole point is
    // that "cannot tell you" and "nothing is there" stay distinguishable. So an
    // `Unsupported` is RECORDED, not failed; anything else is a failure. A slot
    // that silently returned an empty answer instead would be invisible here,
    // which is exactly what issue 0903 looked like.
    let mut unsupported: Vec<&str> = Vec::new();
    macro_rules! classify {
        ($slot:expr, $e:expr) => {{
            if matches!($e, nros::NodeError::Transport(nros::TransportError::Unsupported)) {
                unsupported.push($slot);
                false
            } else {
                failures.push(format!("{}: {:?}", $slot, $e));
                true
            }
        }};
    }

    // Services. A stock talker declares six parameter services, so an empty
    // answer here is a failure rather than a quiet pass.
    let mut services: Vec<String> = Vec::new();
    match executor.get_service_names_and_types(&mut |name, types| {
        services.push(format!("{name} [{}]", types.join(",")));
        true
    }) {
        Ok(()) => {
            services.sort();
            for x in &services {
                println!("GRAPH_SERVICE {x}");
            }
            if !services.iter().any(|s| s.contains(&node)) {
                failures.push(format!(
                    "get_service_names_and_types saw no service of {node:?}: {services:?}"
                ));
            }
        }
        Err(e) => {
            classify!("get_service_names_and_types", e);
        }
    }
    println!("GRAPH_SERVICE_COUNT {}", services.len());

    // Counts on a topic the talker publishes. `count_publishers` must be >= 1;
    // `count_subscribers` legitimately may be 0, so it is checked for an ERROR
    // only — asserting a number nobody guarantees is how a test becomes flaky.
    match executor.count_publishers(topic) {
        Ok(n) => {
            println!("GRAPH_COUNT_PUB {n}");
            if n == 0 {
                failures.push(format!("count_publishers({topic}) == 0 with a talker running"));
            }
        }
        Err(e) => {
            classify!("count_publishers", e);
        }
    }
    match executor.count_subscribers(topic) {
        Ok(n) => println!("GRAPH_COUNT_SUB {n}"),
        Err(e) => {
            classify!("count_subscribers", e);
        }
    }

    // The four by-node forms. Only the publisher one has a guaranteed answer
    // (the talker publishes `/chatter`); the rest must simply not error, which
    // is the assertion that catches an undispatched slot.
    let mut pubs_by_node: Vec<String> = Vec::new();
    match executor.get_publisher_names_and_types_by_node(&node, "/", &mut |name, types| {
        pubs_by_node.push(format!("{name} [{}]", types.join(",")));
        true
    }) {
        Ok(()) => {
            pubs_by_node.sort();
            for x in &pubs_by_node {
                println!("GRAPH_PUB_BY_NODE {x}");
            }
            if !pubs_by_node.iter().any(|t| t.starts_with(topic)) {
                failures.push(format!(
                    "get_publisher_names_and_types_by_node({node}) missing {topic}: {pubs_by_node:?}"
                ));
            }
        }
        Err(e) => {
            classify!("get_publisher_names_and_types_by_node", e);
        }
    }

    let mut n_subs = 0usize;
    if let Err(e) =
        executor.get_subscription_names_and_types_by_node(&node, "/", &mut |name, types| {
            println!("GRAPH_SUB_BY_NODE {name} [{}]", types.join(","));
            n_subs += 1;
            true
        })
    {
        classify!("get_subscription_names_and_types_by_node", e);
    }
    println!("GRAPH_SUB_BY_NODE_COUNT {n_subs}");

    let mut svc_by_node: Vec<String> = Vec::new();
    match executor.get_service_names_and_types_by_node(&node, "/", &mut |name, types| {
        svc_by_node.push(format!("{name} [{}]", types.join(",")));
        true
    }) {
        Ok(()) => {
            svc_by_node.sort();
            for x in &svc_by_node {
                println!("GRAPH_SVC_BY_NODE {x}");
            }
            // The talker's parameter services belong to the talker, so this one
            // does have a guaranteed answer.
            if svc_by_node.is_empty() {
                failures.push(format!(
                    "get_service_names_and_types_by_node({node}) is empty; a node with \
                     parameters always serves some"
                ));
            }
        }
        Err(e) => {
            classify!("get_service_names_and_types_by_node", e);
        }
    }

    let mut n_clients = 0usize;
    if let Err(e) = executor.get_client_names_and_types_by_node(&node, "/", &mut |name, types| {
        println!("GRAPH_CLIENT_BY_NODE {name} [{}]", types.join(","));
        n_clients += 1;
        true
    }) {
        classify!("get_client_names_and_types_by_node", e);
    }
    println!("GRAPH_CLIENT_BY_NODE_COUNT {n_clients}");

    // The two endpoint-info forms. The publisher side must name the talker —
    // this is the slot that answers "who is publishing this, and what type".
    let mut pub_info: Vec<String> = Vec::new();
    match executor.get_publishers_info_by_topic(topic, &mut |info| {
        pub_info.push(format!(
            "{}|{} {} pub={}",
            info.node_namespace, info.node_name, info.topic_type, info.is_publisher
        ));
        true
    }) {
        Ok(()) => {
            pub_info.sort();
            for x in &pub_info {
                println!("GRAPH_PUB_INFO {x}");
            }
            if !pub_info.iter().any(|i| i.contains(&node)) {
                failures.push(format!(
                    "get_publishers_info_by_topic({topic}) does not name {node}: {pub_info:?}"
                ));
            }
        }
        Err(e) => {
            classify!("get_publishers_info_by_topic", e);
        }
    }

    let mut n_sub_info = 0usize;
    if let Err(e) = executor.get_subscriptions_info_by_topic(topic, &mut |info| {
        println!(
            "GRAPH_SUB_INFO {}|{} {}",
            info.node_namespace, info.node_name, info.topic_type
        );
        n_sub_info += 1;
        true
    }) {
        classify!("get_subscriptions_info_by_topic", e);
    }
    println!("GRAPH_SUB_INFO_COUNT {n_sub_info}");

    for slot in &unsupported {
        println!("GRAPH_SLOT_UNSUPPORTED {slot}");
    }
    println!("GRAPH_PROBE_UNSUPPORTED_COUNT {}", unsupported.len());

    if failures.is_empty() && unsupported.is_empty() {
        println!("GRAPH_PROBE_ALL_SLOTS_OK");
    } else if failures.is_empty() {
        // Every failure was a declared "cannot tell you". Honest, and not this
        // probe's business to judge — the CELL decides which backend owes which
        // slots.
        println!("GRAPH_PROBE_SLOTS_PARTIAL");
    } else {
        for f in &failures {
            eprintln!("GRAPH_PROBE_SLOT_FAIL {f}");
        }
        eprintln!("GRAPH_PROBE_FAIL: {} of 11 graph slots failed", failures.len());
        let _ = executor.close();
        std::process::exit(5);
    }

    if let Some(want) = expect_node {
        let hit = settled.iter().any(|n| n.contains(&want));
        if !hit {
            eprintln!(
                "GRAPH_PROBE_FAIL: expected a node matching {want:?}, saw {:?} after {budget_ms} ms",
                settled
            );
            let _ = executor.close();
            std::process::exit(2);
        }
        println!("GRAPH_PROBE_SAW {want}");
    }

    println!("GRAPH_PROBE_DONE");
    let _ = executor.close();
}
