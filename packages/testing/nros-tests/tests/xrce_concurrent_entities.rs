//! Issue 1734 — concurrent entity creation on ONE XRCE session, while another
//! thread drives it.
//!
//! A multi-tier image shares one session across its tier threads, and each
//! tier creates its entities and spins on its own thread. The XRCE shim had no
//! lock: its slot claims were the find-then-mark issue 1711 fixed in the zenoh
//! shim, and every create runs Micro XRCE-DDS-Client (`uxr_buffer_create_*`,
//! `uxr_run_session_until_all_status`) on the shared `uxrSession`, which is not
//! thread-safe. This drives the shim's own entry points — the functions the
//! vtable hands the runtime — from four creating threads and one driving
//! thread, released together by a barrier, against a live Agent.
//!
//! Every create must succeed, and no two live subscriptions may share a slot.
//! The zenoh sibling is
//! `nros-rmw-zenoh/tests/zenoh_integration.rs::concurrent_declares_on_one_session_never_share_a_slot`.
//!
//! Run: `just test-all` (needs the XRCE Agent), beside `xrce_guard_wake`.

#![cfg(feature = "xrce-wake-test")]

use core::ffi::{c_char, c_void};
use std::{
    ffi::CString,
    sync::{
        Arc, Barrier,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

use nros_rmw_cffi::generated::{
    rmw_message_type_support_t, rmw_node_t, rmw_publisher_options_t, rmw_publisher_t,
    rmw_qos_profile_t, rmw_session_t, rmw_subscription_options_t, rmw_subscription_t,
};
use nros_tests::fixtures::{XrceAgent, require_xrce_agent};

// The shim's entry points (packages/rmw/xrce/nros-rmw-xrce/src), linked through
// `nros-rmw-xrce-cffi`. Called directly rather than through `CffiSession`,
// whose own `&mut self` bookkeeping would serialise the test in Rust before it
// reached the code under test.
unsafe extern "C" {
    fn xrce_session_create(
        locator: *const c_char,
        mode: u8,
        domain_id: u32,
        node_name: *const c_char,
        options: *const c_void,
        out: *mut rmw_session_t,
    ) -> i32;
    fn xrce_session_destroy(session: *mut rmw_session_t) -> i32;
    fn xrce_session_drive_io(session: *mut rmw_session_t, timeout_ms: i32) -> i32;
    fn xrce_node_create(
        session: *mut rmw_session_t,
        name: *const c_char,
        namespace_: *const c_char,
        out: *mut rmw_node_t,
    ) -> i32;
    fn xrce_node_destroy(node: *mut rmw_node_t) -> i32;
    fn xrce_publisher_create(
        node: *const rmw_node_t,
        type_support: *const rmw_message_type_support_t,
        topic_name: *const c_char,
        domain_id: u32,
        qos: *const rmw_qos_profile_t,
        options: *const rmw_publisher_options_t,
        out: *mut rmw_publisher_t,
    ) -> i32;
    fn xrce_publisher_destroy(publisher: *mut rmw_publisher_t) -> i32;
    fn xrce_subscription_create(
        node: *const rmw_node_t,
        type_support: *const rmw_message_type_support_t,
        topic_name: *const c_char,
        domain_id: u32,
        qos: *const rmw_qos_profile_t,
        options: *const rmw_subscription_options_t,
        out: *mut rmw_subscription_t,
    ) -> i32;
    fn xrce_subscription_destroy(subscription: *mut rmw_subscription_t) -> i32;
}

/// The leading fields of the shim's `xrce_subscriber_state` (`internal.h`):
/// the session it belongs to, the profile's deadline (issue 2108), then the
/// slot it claimed. Read only to compare slot ADDRESSES — two live
/// subscriptions on one slot is the 1711 race. A field inserted ahead of
/// `slot` in the C struct must be mirrored here, or every subscription reads
/// the same neighbouring field and the test reports a shared slot (issue 2142).
#[repr(C)]
struct SubscriberStatePrefix {
    _session_state: *mut c_void,
    _qos_deadline_ms: u32,
    slot: *mut c_void,
}

const DOMAIN: u32 = 96;
const THREADS: usize = 4;
/// Two subscriptions and one publisher per thread: 8 subscriptions fills
/// `XRCE_MAX_SUBSCRIBERS`' default exactly, so a shared slot cannot hide
/// behind a spare one.
const SUBS_PER_THREAD: usize = 2;
const ROUNDS: usize = 5;

/// A raw pointer the threads may share. The session and node outlive every
/// thread (each round joins them before the next).
#[derive(Clone, Copy)]
struct Shared<T>(*mut T);
unsafe impl<T> Send for Shared<T> {}
unsafe impl<T> Sync for Shared<T> {}

#[test]
fn concurrent_creates_on_one_xrce_session_succeed_on_distinct_slots() {
    require_xrce_agent();
    let agent = XrceAgent::start_unique().expect("start XRCE agent");
    let _ = nros_rmw_xrce_cffi::register();

    let locator = CString::new(agent.addr()).unwrap();
    let session_name = CString::new("xrce_concurrent_entities").unwrap();
    let mut session = rmw_session_t {
        node_name: session_name.as_ptr(),
        namespace_: c"".as_ptr(),
        _reserved: [0; 8],
        backend_data: core::ptr::null_mut(),
    };
    let rc = unsafe {
        xrce_session_create(
            locator.as_ptr(),
            0,
            DOMAIN,
            session_name.as_ptr(),
            core::ptr::null(),
            &mut session,
        )
    };
    assert_eq!(rc, 0, "xrce_session_create");

    let mut node = rmw_node_t {
        name: c"concurrent".as_ptr(),
        namespace_: c"/".as_ptr(),
        session: &mut session,
        _reserved: [0; 8],
        backend_data: core::ptr::null_mut(),
    };
    let rc = unsafe { xrce_node_create(&mut session, node.name, node.namespace_, &mut node) };
    assert_eq!(rc, 0, "xrce_node_create");

    let session_p = Shared(&mut session as *mut rmw_session_t);
    let node_p = Shared(&mut node as *mut rmw_node_t);

    for round in 0..ROUNDS {
        let start = Arc::new(Barrier::new(THREADS + 1));
        let all_created = Arc::new(Barrier::new(THREADS));
        let stop_driving = Arc::new(AtomicBool::new(false));

        // The spinning tier: drives the session the whole time the others
        // create on it, as a tier executor does.
        let driver = {
            let (start, stop) = (start.clone(), stop_driving.clone());
            thread::spawn(move || {
                let session_p = session_p;
                start.wait();
                while !stop.load(Ordering::Acquire) {
                    unsafe { xrce_session_drive_io(session_p.0, 5) };
                }
            })
        };

        let workers: Vec<_> = (0..THREADS)
            .map(|t| {
                let (start, all_created) = (start.clone(), all_created.clone());
                thread::spawn(move || {
                    let node_p = node_p;
                    let ts = rmw_message_type_support_t {
                        type_name: c"std_msgs::msg::dds_::Int32_".as_ptr(),
                        type_hash: c"RIHS01_0000000000000000000000000000000000000000000000000000000000000000".as_ptr(),
                    };
                    let qos: rmw_qos_profile_t = unsafe { core::mem::zeroed() };
                    let sub_opts: rmw_subscription_options_t = unsafe { core::mem::zeroed() };
                    let pub_opts: rmw_publisher_options_t = unsafe { core::mem::zeroed() };
                    let topics: Vec<CString> = (0..=SUBS_PER_THREAD)
                        .map(|i| CString::new(format!("/issue1734/r{round}/t{t}/e{i}")).unwrap())
                        .collect();

                    start.wait();
                    let mut subs: Vec<Box<rmw_subscription_t>> = Vec::new();
                    for topic in &topics[..SUBS_PER_THREAD] {
                        let mut sub: Box<rmw_subscription_t> = Box::new(unsafe { core::mem::zeroed() });
                        let rc = unsafe {
                            xrce_subscription_create(
                                node_p.0, &ts, topic.as_ptr(), DOMAIN, &qos, &sub_opts, &mut *sub,
                            )
                        };
                        assert_eq!(rc, 0, "round {round} t{t}: xrce_subscription_create {topic:?}");
                        subs.push(sub);
                    }
                    let mut publisher: Box<rmw_publisher_t> = Box::new(unsafe { core::mem::zeroed() });
                    let rc = unsafe {
                        xrce_publisher_create(
                            node_p.0,
                            &ts,
                            topics[SUBS_PER_THREAD].as_ptr(),
                            DOMAIN,
                            &qos,
                            &pub_opts,
                            &mut *publisher,
                        )
                    };
                    assert_eq!(rc, 0, "round {round} t{t}: xrce_publisher_create");

                    let slots: Vec<usize> = subs
                        .iter()
                        .map(|s| unsafe { (*(s.backend_data as *const SubscriberStatePrefix)).slot as usize })
                        .collect();
                    // Every thread's entities are live together before any is
                    // destroyed, so a shared slot cannot hide behind a reuse.
                    all_created.wait();
                    for mut sub in subs {
                        assert_eq!(unsafe { xrce_subscription_destroy(&mut *sub) }, 0);
                    }
                    assert_eq!(unsafe { xrce_publisher_destroy(&mut *publisher) }, 0);
                    slots
                })
            })
            .collect();

        let mut slots: Vec<usize> = workers
            .into_iter()
            .flat_map(|w| w.join().expect("a creating thread panicked"))
            .collect();
        stop_driving.store(true, Ordering::Release);
        driver.join().expect("the driving thread panicked");

        slots.sort_unstable();
        let total = slots.len();
        slots.dedup();
        assert_eq!(
            slots.len(),
            total,
            "round {round}: two concurrent creates were handed the same subscriber slot"
        );
    }

    assert_eq!(unsafe { xrce_node_destroy(node_p.0) }, 0);
    assert_eq!(unsafe { xrce_session_destroy(session_p.0) }, 0);
}
