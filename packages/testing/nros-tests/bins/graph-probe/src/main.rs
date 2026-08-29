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

        if !nodes.is_empty() && nodes == previous {
            settled = nodes;
            break;
        }
        previous = nodes;
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
