//! Constrained-link zenoh witness (issue 2163, phase-473 W1/W2).
//!
//! Two behaviours landed in phase-473 and were measured only on the safety
//! island's image; this program makes both observable from a native process
//! talking to a stock `rmw_zenohd`:
//!
//! * **W2** — a TRANSIENT_LOCAL subscription issues one `<key>/@adv/**`
//!   history query, so a late joiner reads a stock latched publisher's value.
//!   The program subscribes AFTER the value was published and prints
//!   `CLP_RECEIVED <n>` per sample, and `CLP_RECEIVED_AT <n> <unix ms>` beside
//!   it. `CLP_DURABILITY=volatile` is the negative control: the same late join
//!   must receive nothing.
//! * **Issue 2366** -- `CLP_ROLE=publish` makes the program a TRANSIENT_LOCAL
//!   publisher instead: it publishes `CLP_VALUE` once, right after creating
//!   the publisher (before the router can have answered its interest), and
//!   `CLP_VALUE2` once more after `CLP_VALUE2_AFTER_MS`, then spins out the
//!   window. A subscription that exists before it must still read the value
//!   (the `@adv` token makes it query the cache), and one whose session
//!   reopens must read a value published while it was away.
//! * **W1** — `graph_discovery = false` compiles the graph cache and its
//!   liveliness subscriber out. After the receive window the program asks for
//!   the domain's node names: an image with discovery ON subscribes to the
//!   graph then and prints `CLP_GRAPH_NODES <count>` (the burst arrived); one
//!   with discovery OFF never declares that subscriber, so the router has
//!   nothing to send it and the query answers `CLP_GRAPH_UNSUPPORTED`.
//!
//! The banner and every marker are printed with `println!` and flushed per
//! line, so the test reads them as they happen.

use std::{
    cell::RefCell,
    io::Write,
    rc::Rc,
    time::{Duration, Instant},
};

use nros::prelude::*;
use std_msgs::msg::Int32;

fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(default)
}

fn say(line: &str) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

fn main() {
    let _ = env_logger::try_init();
    nros_board_linux::register_linked_rmw();

    let topic: &'static str = match std::env::var("CLP_TOPIC") {
        Ok(t) if !t.is_empty() => Box::leak(t.into_boxed_str()),
        _ => "/latched",
    };
    let volatile = std::env::var("CLP_DURABILITY").is_ok_and(|v| v.trim() == "volatile");
    let window = Duration::from_millis(env_u64("CLP_WINDOW_MS", 8_000));
    let graph_budget = Duration::from_millis(env_u64("CLP_GRAPH_MS", 10_000));

    let ctx = nros::init().expect("nros::init failed");
    let cfg = ctx.config("constrained_link_probe");
    let mut executor: Executor = Executor::open(&cfg).expect("open session");
    let nid = executor
        .node_builder("constrained_link_probe")
        .build()
        .expect("build node");

    // KEEP_LAST(1), RELIABLE: the shape of a ROS latched topic. Only the
    // durability differs between the two subscriptions this cell compares.
    let base = QoSProfile::default().keep_last(1).reliable();
    let qos = if volatile {
        base
    } else {
        base.transient_local()
    };
    if std::env::var("CLP_ROLE").is_ok_and(|v| v.trim() == "publish") {
        let value = env_u64("CLP_VALUE", 0) as i32;
        let publisher = executor
            .node_mut(nid)
            .create_publisher_with_qos::<Int32>(topic, qos)
            .expect("create publisher");
        publisher
            .publish(Int32 { data: value })
            .expect("publish");
        say(&format!("CLP_PUBLISHED {value}"));
        // An optional second value, published once at a set time -- while the
        // test holds the router down, for the session re-open case.
        let second = std::env::var("CLP_VALUE2")
            .ok()
            .and_then(|v| v.trim().parse::<i32>().ok());
        let second_at =
            Instant::now() + Duration::from_millis(env_u64("CLP_VALUE2_AFTER_MS", 0));
        let mut second = second;
        let deadline = Instant::now() + window;
        while Instant::now() < deadline {
            executor.spin_once(Duration::from_millis(50));
            if let Some(v) = second
                && Instant::now() >= second_at
            {
                // The link may be down: the publish can fail, and the value
                // must still be retained for the cache to serve it.
                let rc = publisher.publish(Int32 { data: v });
                say(&format!("CLP_PUBLISHED {v} ok={}", rc.is_ok()));
                second = None;
            }
        }
        say("CLP_DONE");
        let _ = executor.close();
        return;
    }
    let received: Rc<RefCell<Vec<i32>>> = Rc::new(RefCell::new(Vec::new()));
    let sink = Rc::clone(&received);
    executor
        .node_mut(nid)
        .subscription(topic)
        .qos(qos)
        .typed::<Int32>()
        .build(move |msg| {
            say(&format!("CLP_RECEIVED {}", msg.data));
            // When, on the host's wall clock, so a test can bound it below by
            // something another process did (issue 2366).
            let unix_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_millis());
            say(&format!("CLP_RECEIVED_AT {} {unix_ms}", msg.data));
            sink.borrow_mut().push(msg.data);
        })
        .expect("create subscription");
    say(&format!(
        "CLP_READY topic={topic} durability={}",
        if volatile {
            "volatile"
        } else {
            "transient_local"
        }
    ));

    let deadline = Instant::now() + window;
    while Instant::now() < deadline {
        executor.spin_once(Duration::from_millis(50));
    }
    say(&format!(
        "CLP_WINDOW_DONE received={}",
        received.borrow().len()
    ));

    // The graph question comes AFTER the receive window, so the history query
    // above is measured on a session that has not yet asked for the graph.
    let graph_deadline = Instant::now() + graph_budget;
    let mut previous = usize::MAX;
    let mut settled = 0usize;
    loop {
        executor.spin_once(Duration::from_millis(200));
        let mut count = 0usize;
        match executor.get_node_names(&mut |_name, _ns, _enclave| {
            count += 1;
            true
        }) {
            Ok(()) => {}
            Err(e) => {
                say(&format!("CLP_GRAPH_UNSUPPORTED err={e:?}"));
                break;
            }
        }
        // Settled = two equal non-zero readings; the budget bounds a graph
        // that keeps changing.
        if count != 0 && count == previous {
            settled = count;
        }
        previous = count;
        if settled != 0 || Instant::now() >= graph_deadline {
            say(&format!("CLP_GRAPH_NODES {}", settled.max(count)));
            break;
        }
    }

    say("CLP_DONE");
    let _ = executor.close();
}
