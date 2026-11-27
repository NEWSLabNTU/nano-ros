//! Drives one managed node through REP-2002 transitions with nano-ros service
//! clients, and counts what it publishes in each state.
//!
//! A test fixture: it lets a host test check an embedded managed node's
//! lifecycle with no ROS 2 install. The node is named by `NROS_LC_NODE`
//! (default `talker`), and its publisher by `NROS_LC_TOPIC` (default
//! `/chatter`, `std_msgs/Int32`). The driver:
//!
//! 1. waits until `/<node>/get_state` answers, then listens for a window;
//! 2. sends `configure`, then listens;
//! 3. sends `activate`, then listens;
//! 4. sends `deactivate`, lets in-flight samples drain, then listens.
//!
//! After each step it prints one line, `LC_PHASE <phase> ok=<bool>
//! state=<label> rx=<count>`, where `ok` is the transition's reply (`true` for
//! the boot step), `state` is what `get_state` answers afterwards, and `rx` is
//! how many messages arrived in the window. Then it prints `LC_DONE`. It
//! asserts nothing itself: the host test reads the lines
//! (`nros_tests::output::LC_PHASE`).

use core::time::Duration;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Instant,
};

use log::error;
use nros::prelude::*;
use nros_lifecycle_msgs::{
    msg::Transition,
    srv::{ChangeState, ChangeStateRequest, GetState, GetStateRequest},
};
use std_msgs::msg::Int32;

/// How long each state is listened to: four periods of the talker's 1 s
/// timer, so an active node publishes at least twice even on a loaded host,
/// and a gated one has four chances to leak.
const WINDOW: Duration = Duration::from_secs(4);
/// What an in-flight sample may take to arrive after `deactivate` replies.
const DRAIN: Duration = Duration::from_millis(700);
/// How long one service call may take.
const REPLY: Duration = Duration::from_secs(5);
/// How long the node may take to come up.
const BOOT: Duration = Duration::from_secs(90);

fn env_or(name: &str, default: &'static str) -> &'static str {
    match std::env::var(name) {
        Ok(v) if !v.is_empty() => Box::leak(v.into_boxed_str()),
        _ => default,
    }
}

/// Spin for `d`, sleeping between passes so the host thread does not busy-wait.
fn spin_for(executor: &mut Executor, d: Duration) {
    let end = Instant::now() + d;
    while Instant::now() < end {
        executor.spin_once(Duration::from_millis(20));
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

/// Spin until `done` holds or `limit` passes. True when it held.
fn spin_until(executor: &mut Executor, limit: Duration, done: impl Fn() -> bool) -> bool {
    let end = Instant::now() + limit;
    while Instant::now() < end {
        if done() {
            return true;
        }
        executor.spin_once(Duration::from_millis(20));
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    done()
}

fn run() -> i32 {
    let node = env_or("NROS_LC_NODE", "talker");
    let topic = env_or("NROS_LC_TOPIC", "/chatter");
    let change_state: &'static str = Box::leak(format!("/{node}/change_state").into_boxed_str());
    let get_state: &'static str = Box::leak(format!("/{node}/get_state").into_boxed_str());

    let ctx = nros::init().expect("nros::init failed");
    let cfg = ctx.config("lifecycle_driver");
    let mut executor: Executor = Executor::open(&cfg).expect("Failed to open session");
    let nid = executor
        .node_builder("lifecycle_driver")
        .build()
        .expect("Failed to build node");

    let rx = Rc::new(Cell::new(0u32));
    let rx_cb = rx.clone();
    executor
        .node_mut(nid)
        .subscription(topic)
        .typed::<Int32>()
        .build(move |_msg| rx_cb.set(rx_cb.get() + 1))
        .expect("Failed to add subscription");

    let state: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    let state_cb = state.clone();
    let mut get = executor
        .node_mut(nid)
        .create_client_with_callback::<GetState, _>(get_state, move |reply| {
            *state_cb.borrow_mut() = Some(reply.current_state.label.as_str().to_owned());
        })
        .expect("Failed to create the get_state client");

    let changed: Rc<Cell<Option<bool>>> = Rc::new(Cell::new(None));
    let changed_cb = changed.clone();
    let mut change = executor
        .node_mut(nid)
        .create_client_with_callback::<ChangeState, _>(change_state, move |reply| {
            changed_cb.set(Some(reply.success));
        })
        .expect("Failed to create the change_state client");

    // Ask get_state until the node answers: the image may still be booting.
    let mut query = |executor: &mut Executor, limit: Duration| -> String {
        *state.borrow_mut() = None;
        if get.call(&GetStateRequest {}).is_err() {
            return String::from("<send failed>");
        }
        let probe = state.clone();
        if spin_until(executor, limit, || probe.borrow().is_some()) {
            state.borrow().clone().unwrap_or_default()
        } else {
            String::from("<no reply>")
        }
    };
    let boot_end = Instant::now() + BOOT;
    let mut boot_state = String::from("<no reply>");
    while Instant::now() < boot_end {
        boot_state = query(&mut executor, Duration::from_secs(2));
        if !boot_state.starts_with('<') {
            break;
        }
    }
    if boot_state.starts_with('<') {
        error!("{get_state} never answered within {BOOT:?}");
        println!("LC_PHASE boot ok=false state={boot_state} rx=0");
        println!("LC_DONE");
        return 1;
    }

    rx.set(0);
    spin_for(&mut executor, WINDOW);
    let boot_state = query(&mut executor, REPLY);
    println!("LC_PHASE boot ok=true state={boot_state} rx={}", rx.get());

    for (phase, id) in [
        ("configure", Transition::TRANSITION_CONFIGURE),
        ("activate", Transition::TRANSITION_ACTIVATE),
        ("deactivate", Transition::TRANSITION_DEACTIVATE),
    ] {
        changed.set(None);
        let request = ChangeStateRequest {
            transition: Transition {
                id,
                ..Default::default()
            },
        };
        let sent = change.call(&request).is_ok();
        let probe = changed.clone();
        let replied = sent && spin_until(&mut executor, REPLY, || probe.get().is_some());
        let ok = replied && changed.get() == Some(true);
        if phase == "deactivate" {
            spin_for(&mut executor, DRAIN);
        }
        rx.set(0);
        spin_for(&mut executor, WINDOW);
        let after = query(&mut executor, REPLY);
        println!("LC_PHASE {phase} ok={ok} state={after} rx={}", rx.get());
    }
    println!("LC_DONE");
    0
}

fn main() {
    nros_board_linux::register_linked_rmw();
    env_logger::init();
    std::process::exit(run());
}
