//! Phase 376 W5/B1 — the shim calls `create_node` once per distinct node.
//!
//! Before this the shim had no node concept: `CffiSession::entity_view`
//! fabricated a per-call session whose `node_name` carried the entity's owning
//! node, and each backend re-derived the set of nodes from that string. The
//! contract now is that the runtime owns the question — which is only
//! meaningful if it actually deduplicates.
#![cfg(feature = "alloc")]

use core::{
    ffi::c_char,
    sync::atomic::{AtomicUsize, Ordering},
};

use nros_rmw::{QoSProfile, Session as _, TopicInfo};
use nros_rmw_cffi::{
    CffiSession, EMPTY_VTABLE, NROS_RMW_RET_OK, NrosRmwNode, NrosRmwPublisher, NrosRmwQos,
    NrosRmwSession, NrosRmwSessionOptions, NrosRmwVtable, generated, nros_rmw_cffi_register_named,
};

// The call counters below are file-globals, so the tests in this binary must
// not run concurrently — one test's `create_node` would be counted by another.
static GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());

static CREATE_NODE_CALLS: AtomicUsize = AtomicUsize::new(0);
/// Every name `create_node` was handed, in order, as a fixed-size log so the
/// test can assert WHICH nodes were announced and not merely how many.
static SEEN: [AtomicUsize; 4] = [
    AtomicUsize::new(0),
    AtomicUsize::new(0),
    AtomicUsize::new(0),
    AtomicUsize::new(0),
];

unsafe fn first_byte(p: *const c_char) -> usize {
    if p.is_null() {
        0
    } else {
        // Read the byte through `u8` rather than `c_char`: `c_char` is signed on
        // x86_64 and unsigned on aarch64, so neither literal spelling is portable.
        unsafe { *p.cast::<u8>() as usize }
    }
}

unsafe extern "C" fn stub_create_node(
    _session: *mut NrosRmwSession,
    name: *const c_char,
    _namespace_: *const c_char,
    out: *mut NrosRmwNode,
) -> i32 {
    let n = CREATE_NODE_CALLS.fetch_add(1, Ordering::SeqCst);
    if n < SEEN.len() {
        SEEN[n].store(unsafe { first_byte(name) }, Ordering::SeqCst);
    }
    // A backend writes its own state here; a non-null value proves the shim
    // carries it back into the node it hands the create_* slots.
    unsafe { (*out).backend_data = 0x0DE5_usize as *mut core::ffi::c_void };
    NROS_RMW_RET_OK
}

static DESTROY_NODE_CALLS: AtomicUsize = AtomicUsize::new(0);
static DESTROY_NODE_BACKEND_DATA: AtomicUsize = AtomicUsize::new(0);

unsafe extern "C" fn stub_destroy_node(node: *mut NrosRmwNode) -> i32 {
    DESTROY_NODE_CALLS.fetch_add(1, Ordering::SeqCst);
    DESTROY_NODE_BACKEND_DATA.store(unsafe { (*node).backend_data } as usize, Ordering::SeqCst);
    NROS_RMW_RET_OK
}

static LAST_PUBLISH_NODE: AtomicUsize = AtomicUsize::new(0);
static LAST_NODE_BACKEND_DATA: AtomicUsize = AtomicUsize::new(0);

#[allow(clippy::too_many_arguments)]
unsafe extern "C" fn stub_create_publisher(
    node: *const NrosRmwNode,
    _topic: *const c_char,
    _ty: *const c_char,
    _hash: *const c_char,
    _domain: u32,
    _qos: *const NrosRmwQos,
    _opts: *const generated::rmw_publisher_options_t,
    out: *mut NrosRmwPublisher,
) -> i32 {
    unsafe {
        LAST_PUBLISH_NODE.store(first_byte((*node).name), Ordering::SeqCst);
        LAST_NODE_BACKEND_DATA.store((*node).backend_data as usize, Ordering::SeqCst);
        // The node must carry a live route to its session — that field is our
        // `context`, and without it a backend cannot reach its own state.
        assert!(!(*node).session.is_null(), "node.session must not be null");
        // Any non-null value: the shim only checks for NULL here.
        (*out).backend_data = core::ptr::dangling_mut::<core::ffi::c_void>();
    }
    NROS_RMW_RET_OK
}

unsafe extern "C" fn stub_open(
    _loc: *const c_char,
    _mode: u8,
    _domain: u32,
    _name: *const c_char,
    _options: *const NrosRmwSessionOptions,
    out: *mut NrosRmwSession,
) -> i32 {
    unsafe { (*out).backend_data = 0x5E55_usize as *mut core::ffi::c_void };
    NROS_RMW_RET_OK
}

unsafe extern "C" fn stub_close(_s: *mut NrosRmwSession) -> i32 {
    NROS_RMW_RET_OK
}
unsafe extern "C" fn stub_drive(_s: *mut NrosRmwSession, _t: i32) -> i32 {
    NROS_RMW_RET_OK
}
unsafe extern "C" fn stub_dpub(_p: *mut NrosRmwPublisher) -> i32 {
    NROS_RMW_RET_OK
}

/// The registry refuses a vtable missing a required slot (issue 0349), so the
/// slots this test does not exercise still have to exist.
mod fill {
    use core::ffi::c_char;

    use nros_rmw_cffi::{NROS_RMW_RET_OK, generated::*};

    #[allow(clippy::too_many_arguments)]
    pub unsafe extern "C" fn csub(
        _: *const rmw_node_t,
        _: *const c_char,
        _: *const c_char,
        _: *const c_char,
        _: u32,
        _: *const rmw_qos_profile_t,
        _: *const rmw_subscription_options_t,
        _: *mut rmw_subscription_t,
    ) -> rmw_ret_t {
        NROS_RMW_RET_OK
    }
    pub unsafe extern "C" fn dsub(_: *mut rmw_subscription_t) -> rmw_ret_t {
        NROS_RMW_RET_OK
    }
    pub unsafe extern "C" fn publish(
        _: *const rmw_publisher_t,
        _: *const u8,
        _: usize,
    ) -> rmw_ret_t {
        NROS_RMW_RET_OK
    }
    pub unsafe extern "C" fn hasd(_: *mut rmw_subscription_t, t: *mut bool) -> rmw_ret_t {
        unsafe { *t = false };
        NROS_RMW_RET_OK
    }
    pub unsafe extern "C" fn take(
        _: *const rmw_subscription_t,
        _: *mut u8,
        _: usize,
        _: *mut usize,
        t: *mut bool,
    ) -> rmw_ret_t {
        unsafe { *t = false };
        NROS_RMW_RET_OK
    }
    pub unsafe extern "C" fn csrv(
        _: *const rmw_node_t,
        _: *const c_char,
        _: *const c_char,
        _: *const c_char,
        _: u32,
        _: *const rmw_qos_profile_t,
        _: *mut rmw_service_t,
    ) -> rmw_ret_t {
        NROS_RMW_RET_OK
    }
    pub unsafe extern "C" fn dsrv(_: *mut rmw_service_t) -> rmw_ret_t {
        NROS_RMW_RET_OK
    }
    pub unsafe extern "C" fn ccli(
        _: *const rmw_node_t,
        _: *const c_char,
        _: *const c_char,
        _: *const c_char,
        _: u32,
        _: *const rmw_qos_profile_t,
        _: *mut rmw_client_t,
    ) -> rmw_ret_t {
        NROS_RMW_RET_OK
    }
    pub unsafe extern "C" fn dcli(_: *mut rmw_client_t) -> rmw_ret_t {
        NROS_RMW_RET_OK
    }
    pub unsafe extern "C" fn sresp(
        _: *const rmw_service_t,
        _: i64,
        _: *const u8,
        _: usize,
    ) -> rmw_ret_t {
        NROS_RMW_RET_OK
    }
    pub unsafe extern "C" fn hasr(_: *mut rmw_service_t, h: *mut bool) -> rmw_ret_t {
        unsafe { *h = false };
        NROS_RMW_RET_OK
    }
    pub unsafe extern "C" fn takereq(
        _: *const rmw_service_t,
        _: *mut u8,
        _: usize,
        _: *mut i64,
        _: *mut usize,
        t: *mut bool,
    ) -> rmw_ret_t {
        unsafe { *t = false };
        NROS_RMW_RET_OK
    }
}

static VT: NrosRmwVtable = NrosRmwVtable {
    create_session: Some(stub_open),
    destroy_session: Some(stub_close),
    drive_io: Some(stub_drive),
    create_node: Some(stub_create_node),
    destroy_node: Some(stub_destroy_node),
    create_publisher: Some(stub_create_publisher),
    destroy_publisher: Some(stub_dpub),
    create_subscription: Some(fill::csub),
    destroy_subscription: Some(fill::dsub),
    publish: Some(fill::publish),
    has_data: Some(fill::hasd),
    take: Some(fill::take),
    create_service: Some(fill::csrv),
    destroy_service: Some(fill::dsrv),
    create_client: Some(fill::ccli),
    destroy_client: Some(fill::dcli),
    send_response: Some(fill::sresp),
    has_request: Some(fill::hasr),
    take_request: Some(fill::takereq),
    ..EMPTY_VTABLE
};

fn topic_on<'a>(node: &'a str, name: &'a str) -> TopicInfo<'a> {
    TopicInfo {
        name,
        type_name: "std_msgs/msg/Int32",
        type_hash: "",
        domain_id: 0,
        node_name: Some(node),
        namespace: "/",
        rx_buffer_hint: 0,
        tx_express: false,
    }
}

#[test]
fn create_node_fires_once_per_distinct_node() {
    let _g = GUARD.lock().unwrap_or_else(|e| e.into_inner());
    // Counters are cumulative across this binary's tests and `SEEN` is indexed
    // by the call count, so both start from a known state under the guard.
    CREATE_NODE_CALLS.store(0, Ordering::SeqCst);
    for cell in SEEN.iter() {
        cell.store(0, Ordering::SeqCst);
    }
    let rc = unsafe { nros_rmw_cffi_register_named(c"node-slot".as_ptr(), &VT) };
    assert_eq!(rc, NROS_RMW_RET_OK);
    let mut session =
        CffiSession::open_named("node-slot", "", 0, 0, "fallback").expect("open_named");

    // Two entities on ONE node: the slot fires once.
    let _p1 = session
        .create_publisher(&topic_on("alpha", "/a"), QoSProfile::default())
        .expect("first publisher");
    assert_eq!(CREATE_NODE_CALLS.load(Ordering::SeqCst), 1);
    let _p2 = session
        .create_publisher(&topic_on("alpha", "/b"), QoSProfile::default())
        .expect("second publisher on the same node");
    assert_eq!(
        CREATE_NODE_CALLS.load(Ordering::SeqCst),
        1,
        "a second entity on the same node must REUSE the node record"
    );

    // A different node is a different record.
    let _p3 = session
        .create_publisher(&topic_on("beta", "/c"), QoSProfile::default())
        .expect("publisher on a second node");
    assert_eq!(CREATE_NODE_CALLS.load(Ordering::SeqCst), 2);

    assert_eq!(SEEN[0].load(Ordering::SeqCst), b'a' as usize);
    assert_eq!(SEEN[1].load(Ordering::SeqCst), b'b' as usize);
    assert_eq!(
        LAST_PUBLISH_NODE.load(Ordering::SeqCst),
        b'b' as usize,
        "create_publisher must receive the node the entity belongs to"
    );
    assert_eq!(
        LAST_NODE_BACKEND_DATA.load(Ordering::SeqCst),
        0x0DE5_usize,
        "the backend_data create_node wrote must reach create_publisher"
    );
}

/// Issue 0800 — `create_node` was dispatched and `destroy_node` never was, so a
/// backend that allocated state in the create never heard that the node was
/// gone. Nothing caught it: the slot had no producer AND no consumer, which
/// looks exactly like an optional slot nobody needs.
#[test]
fn closing_a_session_destroys_the_nodes_it_created() {
    let _g = GUARD.lock().unwrap_or_else(|e| e.into_inner());
    let rc = unsafe { nros_rmw_cffi_register_named(c"node-destroy".as_ptr(), &VT) };
    assert_eq!(rc, NROS_RMW_RET_OK);

    let before = DESTROY_NODE_CALLS.load(Ordering::SeqCst);
    {
        let mut session =
            CffiSession::open_named("node-destroy", "", 0, 0, "fallback").expect("open_named");
        let _p = session
            .create_publisher(&topic_on("gamma", "/g"), QoSProfile::default())
            .expect("publisher");
        let _q = session
            .create_publisher(&topic_on("delta", "/d"), QoSProfile::default())
            .expect("publisher on a second node");
        assert_eq!(
            DESTROY_NODE_CALLS.load(Ordering::SeqCst),
            before,
            "nothing is destroyed while the session is open"
        );
        session.close().expect("close");
    }

    assert_eq!(
        DESTROY_NODE_CALLS.load(Ordering::SeqCst) - before,
        2,
        "each node the session created must be handed back to the backend"
    );
    assert_eq!(
        DESTROY_NODE_BACKEND_DATA.load(Ordering::SeqCst),
        0x0DE5_usize,
        "destroy_node must receive the backend_data create_node wrote, not a null view"
    );
}
