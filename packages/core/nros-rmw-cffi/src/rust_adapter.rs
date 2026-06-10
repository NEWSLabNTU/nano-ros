//! Phase 115.L.0 — Generic Rust-trait → C-vtable adapter.
//!
//! `RustBackendAdapter<R>` converts any `R: nros_rmw::Rmw` whose
//! associated types implement the matching `Session` / `Publisher` /
//! `Subscriber` / `ServiceServerTrait` / `ServiceClientTrait` traits
//! into a `static NrosRmwVtable`. Each per-backend cffi crate then
//! collapses to ~10 LOC:
//!
//! ```ignore
//! #[unsafe(no_mangle)]
//! pub extern "C" fn nros_rmw_zenoh_register() -> nros_rmw_cffi::NrosRmwRet {
//!     nros_rmw_cffi::RustBackendAdapter::<nros_rmw_zenoh::ZenohRmw>::register()
//! }
//! ```
//!
//! # Storage discipline
//!
//! - Session: `Box::into_raw(Box::new(session))` is stashed in
//!   `NrosRmwSession::backend_data`. `close` reclaims via
//!   `Box::from_raw` (drops the box, runs `Drop`, frees the alloc).
//! - Publisher / Subscriber / ServiceServer / ServiceClient: same
//!   pattern with their respective handle types.
//!
//! # 'static
//!
//! Every handle must be `'static` (the `Box` outlives the call). We
//! deliberately do **not** require `Send`: the C runtime hands the
//! `backend_data` pointer back to the same caller that minted it,
//! and the executor's single-thread-per-session invariant matches
//! the Rust trait surface. Zenoh-pico's `ZenohSession`, for
//! instance, holds an unmovable `*const Context` which the
//! upstream library does not mark `Send`.
//!
//! # Bounds
//!
//! All error types must be `TransportError` (or `Into<TransportError>`).
//! This matches every in-tree backend today.

extern crate alloc;

use alloc::boxed::Box;
use core::{ffi::c_void, marker::PhantomData};

use nros_rmw::{
    Publisher, QosSettings, Rmw, RmwConfig, ServiceClientTrait, ServiceServerTrait, Session,
    SessionMode, Subscriber, TopicInfo, TransportError,
};

use crate::{
    NROS_RMW_RET_INVALID_ARGUMENT, NROS_RMW_RET_NO_DATA, NROS_RMW_RET_OK, NROS_RMW_RET_UNSUPPORTED,
    NrosRmwEventCallback, NrosRmwEventKind, NrosRmwPublisher, NrosRmwQos, NrosRmwRet,
    NrosRmwServiceClient, NrosRmwServiceServer, NrosRmwSession, NrosRmwSubscriber, NrosRmwVtable,
    ret_from_error,
};

#[cfg(all(target_os = "none", not(feature = "std")))]
mod static_subscriber_storage {
    use core::{cell::UnsafeCell, mem, ptr};

    use portable_atomic::{AtomicBool, Ordering};

    const SLOT_COUNT: usize = 4;
    const SLOT_SIZE: usize = 1024;
    const SLOT_ALIGN: usize = 16;

    #[repr(align(16))]
    struct Slot {
        bytes: UnsafeCell<[u8; SLOT_SIZE]>,
    }

    // Phase 192.5 — `#[repr(align(16))]` can't take the `SLOT_ALIGN` const, so
    // assert they stay in lockstep (the insert() guard compares against SLOT_ALIGN).
    const _: () = assert!(mem::align_of::<Slot>() == SLOT_ALIGN);

    unsafe impl Sync for Slot {}

    impl Slot {
        const fn new() -> Self {
            Self {
                bytes: UnsafeCell::new([0; SLOT_SIZE]),
            }
        }
    }

    static USED: [AtomicBool; SLOT_COUNT] = [const { AtomicBool::new(false) }; SLOT_COUNT];
    static SLOTS: [Slot; SLOT_COUNT] = [const { Slot::new() }; SLOT_COUNT];

    pub unsafe fn insert<T>(value: T) -> Option<*mut T> {
        if mem::size_of::<T>() > SLOT_SIZE || mem::align_of::<T>() > SLOT_ALIGN {
            return None;
        }

        for index in 0..SLOT_COUNT {
            if USED[index]
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
            {
                continue;
            }

            let ptr = SLOTS[index].bytes.get().cast::<T>();
            unsafe { ptr.write(value) };
            return Some(ptr);
        }

        None
    }

    pub unsafe fn take<T>(ptr: *mut T) -> bool {
        if ptr.is_null() {
            return false;
        }

        for index in 0..SLOT_COUNT {
            let slot_ptr = SLOTS[index].bytes.get().cast::<T>();
            if ptr != slot_ptr {
                continue;
            }

            unsafe { ptr::drop_in_place(ptr) };
            USED[index].store(false, Ordering::Release);
            return true;
        }

        false
    }
}

// ============================================================================
// Trait alias bundle
// ============================================================================
//
// `RustBackend` ties together every constraint the adapter needs. The
// alternative — long `where` clauses on every fn — is unreadable. This
// trait is sealed: backends don't impl it directly; the blanket impl
// below picks it up automatically for any `R: Rmw` whose associated
// types line up.

/// Bundle of trait bounds an `Rmw` backend must satisfy to be exposed
/// through [`RustBackendAdapter`]. Implemented automatically for any
/// `R: Rmw` whose handle types use `TransportError` and are
/// `'static`.
pub trait RustBackend: Sized {
    type Session: Session<
            Error = TransportError,
            PublisherHandle = Self::Publisher,
            SubscriberHandle = Self::Subscriber,
            ServiceServerHandle = Self::ServiceServer,
            ServiceClientHandle = Self::ServiceClient,
        > + 'static;
    type Publisher: Publisher<Error = TransportError> + 'static;
    type Subscriber: Subscriber<Error = TransportError> + 'static;
    type ServiceServer: ServiceServerTrait<Error = TransportError> + 'static;
    type ServiceClient: ServiceClientTrait<Error = TransportError> + 'static;

    /// Construct a fresh factory instance. Called inside the `open`
    /// trampoline. Equivalent to `R::default()` for backends that
    /// `derive(Default)`; the indirection keeps the door open for
    /// future per-backend registration knobs.
    fn factory() -> Self;

    /// Move the factory into a session per the `Rmw::open` contract.
    fn open(self, config: &RmwConfig) -> Result<Self::Session, TransportError>;
}

impl<R> RustBackend for R
where
    R: Rmw<Error = TransportError> + Default + Sized,
    R::Session: Session<Error = TransportError> + 'static,
    <R::Session as Session>::PublisherHandle: Publisher<Error = TransportError> + 'static,
    <R::Session as Session>::SubscriberHandle: Subscriber<Error = TransportError> + 'static,
    <R::Session as Session>::ServiceServerHandle:
        ServiceServerTrait<Error = TransportError> + 'static,
    <R::Session as Session>::ServiceClientHandle:
        ServiceClientTrait<Error = TransportError> + 'static,
{
    type Session = R::Session;
    type Publisher = <R::Session as Session>::PublisherHandle;
    type Subscriber = <R::Session as Session>::SubscriberHandle;
    type ServiceServer = <R::Session as Session>::ServiceServerHandle;
    type ServiceClient = <R::Session as Session>::ServiceClientHandle;

    fn factory() -> Self {
        R::default()
    }

    fn open(self, config: &RmwConfig) -> Result<Self::Session, TransportError> {
        Rmw::open(self, config)
    }
}

// ============================================================================
// Helpers: C-string <-> &str
// ============================================================================

/// Read a null-terminated C string into a Rust `&str`. Returns the
/// empty string if `ptr` is null or contains invalid UTF-8.
///
/// # Safety
///
/// `ptr`, if non-null, must point to a valid null-terminated byte
/// sequence that outlives the returned borrow.
unsafe fn cstr_to_str<'a>(ptr: *const u8) -> &'a str {
    if ptr.is_null() {
        return "";
    }
    let mut len = 0usize;
    // Bound the scan so a missing terminator can't read off the end of
    // a small caller buffer. 1 KiB matches the typical NAME_BUF_LEN
    // (256) plus headroom for type-name / hash strings.
    while len < 4096 && unsafe { *ptr.add(len) } != 0 {
        len += 1;
    }
    let slice = unsafe { core::slice::from_raw_parts(ptr, len) };
    core::str::from_utf8(slice).unwrap_or("")
}

/// Convert the cffi QoS view back into a `nros_rmw::QosSettings`. The
/// adapter trampolines call this when forwarding `create_publisher` /
/// `create_subscriber` into the Rust trait.
fn qos_from_cffi(q: &NrosRmwQos) -> QosSettings {
    use nros_rmw::{
        QosDurabilityPolicy, QosHistoryPolicy, QosLivelinessPolicy, QosReliabilityPolicy,
    };
    QosSettings {
        reliability: if q.reliability == 0 {
            QosReliabilityPolicy::BestEffort
        } else {
            QosReliabilityPolicy::Reliable
        },
        durability: if q.durability == 0 {
            QosDurabilityPolicy::Volatile
        } else {
            QosDurabilityPolicy::TransientLocal
        },
        history: if q.history == 0 {
            QosHistoryPolicy::KeepLast
        } else {
            QosHistoryPolicy::KeepAll
        },
        depth: q.depth as u32,
        deadline_ms: q.deadline_ms,
        lifespan_ms: q.lifespan_ms,
        liveliness_kind: match q.liveliness_kind {
            1 => QosLivelinessPolicy::Automatic,
            2 => QosLivelinessPolicy::ManualByTopic,
            3 => QosLivelinessPolicy::ManualByNode,
            _ => QosLivelinessPolicy::None,
        },
        liveliness_lease_ms: q.liveliness_lease_ms,
        avoid_ros_namespace_conventions: q.avoid_ros_namespace_conventions != 0,
    }
}

unsafe fn session_node_name<'a>(session: *const NrosRmwSession) -> Option<&'a str> {
    let name = unsafe { cstr_to_str((*session).node_name) };
    if name.is_empty() { None } else { Some(name) }
}

unsafe fn session_namespace<'a>(session: *const NrosRmwSession) -> &'a str {
    let namespace = unsafe { cstr_to_str((*session).namespace_) };
    if namespace.is_empty() { "/" } else { namespace }
}

// ============================================================================
// Adapter
// ============================================================================

/// Wraps a Rust `Rmw` backend behind the canonical
/// [`NrosRmwVtable`] C ABI. See module docs.
pub struct RustBackendAdapter<R>(PhantomData<R>);

impl<R: RustBackend> RustBackendAdapter<R> {
    /// Monomorphised vtable for backend `R`. The `const` is promoted
    /// to per-type static storage, so `&Self::VTABLE` has `'static`
    /// lifetime — safe to hand to `nros_rmw_cffi_register`.
    pub const VTABLE: NrosRmwVtable = NrosRmwVtable {
        open: open_trampoline::<R>,
        close: close_trampoline::<R>,
        drive_io: drive_io_trampoline::<R>,
        create_publisher: create_publisher_trampoline::<R>,
        destroy_publisher: destroy_publisher_trampoline::<R>,
        publish_raw: publish_raw_trampoline::<R>,
        create_subscriber: create_subscriber_trampoline::<R>,
        destroy_subscriber: destroy_subscriber_trampoline::<R>,
        try_recv_raw: try_recv_raw_trampoline::<R>,
        has_data: has_data_trampoline::<R>,
        create_service_server: create_service_server_trampoline::<R>,
        destroy_service_server: destroy_service_server_trampoline::<R>,
        try_recv_request: try_recv_request_trampoline::<R>,
        has_request: has_request_trampoline::<R>,
        send_reply: send_reply_trampoline::<R>,
        create_service_client: create_service_client_trampoline::<R>,
        destroy_service_client: destroy_service_client_trampoline::<R>,
        call_raw: call_raw_trampoline::<R>,
        // Phase 130.8 — wire non-blocking trampolines so Rust-backed
        // CFFI consumers (dust-DDS, future Rust Cyclone wrapper)
        // skip the legacy blocking call_raw fallback inside
        // CffiServiceClient. Backends that don't override the trait
        // defaults inherit the "store pending + map NoData to
        // Ok(None)" base behaviour.
        send_request_raw: Some(send_request_raw_trampoline::<R>),
        try_recv_reply_raw: Some(try_recv_reply_raw_trampoline::<R>),
        register_subscriber_event: register_subscriber_event_trampoline::<R>,
        register_publisher_event: register_publisher_event_trampoline::<R>,
        assert_publisher_liveliness: assert_publisher_liveliness_trampoline::<R>,
        next_deadline_ms: Some(next_deadline_ms_trampoline::<R>),
        set_wake_callback: Some(set_wake_callback_trampoline::<R>),
        // Phase 124.A — zero-copy slots default to NULL on the
        // generic adapter; per-backend opt-in via dedicated trampolines
        // (see `nros-rmw-zenoh` for the first implementation in 124.A.4).
        // Runtime falls back to the arena path when these are NULL.
        pub_loan: None,
        pub_commit: None,
        pub_discard: None,
        sub_borrow: None,
        sub_release: None,
        service_server_available: Some(service_server_available_trampoline::<R>),
        try_recv_sequence: Some(try_recv_sequence_trampoline::<R>),
        publish_streamed: Some(publish_streamed_trampoline::<R>),
        ping_session: Some(ping_session_trampoline::<R>),
        subscriber_supports_in_place: Some(subscriber_supports_in_place_trampoline::<R>),
        process_raw_in_place: Some(process_raw_in_place_trampoline::<R>),
    };

    /// Install the per-`R` vtable into the cffi registry under the
    /// implicit name `"default"`. Idempotent — re-registering the
    /// same vtable is a no-op from the runtime's perspective.
    ///
    /// Most backends should use [`register_named`](Self::register_named)
    /// instead so they show up in the registry under their canonical
    /// name (`"zenoh"`, `"dds"`, `"xrce"`, …). Phase 128.B.5 routes
    /// the implicit-name path through `_register_named` too, so the
    /// legacy unnamed C shim no longer participates in registration.
    pub fn register() -> NrosRmwRet {
        // SAFETY: `&Self::VTABLE` is a reference to a const-promoted
        // static; address stable for the program's lifetime.
        unsafe { crate::nros_rmw_cffi_register_named(c"default".as_ptr(), &Self::VTABLE) }
    }

    /// Phase 104.B.2 — install the per-`R` vtable under a stable
    /// name. Multiple backends can coexist via this entry point.
    ///
    /// # Safety
    /// `name` must be a valid NUL-terminated UTF-8 string.
    pub unsafe fn register_named(name: *const core::ffi::c_char) -> NrosRmwRet {
        // SAFETY: `&Self::VTABLE` is a reference to a const-promoted
        // static; address stable for the program's lifetime.
        unsafe { crate::nros_rmw_cffi_register_named(name, &Self::VTABLE) }
    }
}

// ============================================================================
// Trampolines — session lifecycle
// ============================================================================

unsafe extern "C" fn open_trampoline<R: RustBackend>(
    locator: *const u8,
    mode: u8,
    domain_id: u32,
    node_name: *const u8,
    out: *mut NrosRmwSession,
) -> NrosRmwRet {
    if out.is_null() {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    let cfg = RmwConfig {
        locator: unsafe { cstr_to_str(locator) },
        mode: if mode == 0 {
            SessionMode::Client
        } else {
            SessionMode::Peer
        },
        domain_id,
        node_name: unsafe { cstr_to_str(node_name) },
        namespace: "",
        properties: &[],
    };
    let factory = R::factory();
    match factory.open(&cfg) {
        Ok(session) => {
            let boxed = Box::into_raw(Box::new(session));
            unsafe {
                (*out).backend_data = boxed as *mut c_void;
            }
            NROS_RMW_RET_OK
        }
        Err(e) => ret_from_error(&e),
    }
}

unsafe extern "C" fn close_trampoline<R: RustBackend>(session: *mut NrosRmwSession) -> NrosRmwRet {
    let Some(boxed) = (unsafe { take_box::<R::Session>(session_data_mut(session)) }) else {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    };
    let mut s = boxed;
    let ret = match Session::close(&mut *s) {
        Ok(()) => NROS_RMW_RET_OK,
        Err(e) => ret_from_error(&e),
    };
    drop(s);
    ret
}

unsafe extern "C" fn drive_io_trampoline<R: RustBackend>(
    session: *mut NrosRmwSession,
    timeout_ms: i32,
) -> NrosRmwRet {
    let Some(s) = (unsafe { session_mut::<R::Session>(session) }) else {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    };
    match Session::drive_io(s, timeout_ms) {
        Ok(()) => NROS_RMW_RET_OK,
        Err(e) => ret_from_error(&e),
    }
}

// ============================================================================
// Trampolines — publisher
// ============================================================================

unsafe extern "C" fn create_publisher_trampoline<R: RustBackend>(
    session: *mut NrosRmwSession,
    topic_name: *const u8,
    type_name: *const u8,
    type_hash: *const u8,
    domain_id: u32,
    qos: *const NrosRmwQos,
    out: *mut NrosRmwPublisher,
) -> NrosRmwRet {
    if out.is_null() || qos.is_null() {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    let Some(s) = (unsafe { session_mut::<R::Session>(session) }) else {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    };
    let node_name = unsafe { session_node_name(session) };
    let namespace = unsafe { session_namespace(session) };
    let topic = TopicInfo {
        name: unsafe { cstr_to_str(topic_name) },
        type_name: unsafe { cstr_to_str(type_name) },
        type_hash: unsafe { cstr_to_str(type_hash) },
        domain_id,
        node_name,
        namespace,
    };
    let qos_settings = qos_from_cffi(unsafe { &*qos });
    match Session::create_publisher(s, &topic, qos_settings) {
        Ok(pub_handle) => {
            let boxed = Box::into_raw(Box::new(pub_handle));
            unsafe {
                (*out).backend_data = boxed as *mut c_void;
            }
            NROS_RMW_RET_OK
        }
        Err(e) => ret_from_error(&e),
    }
}

unsafe extern "C" fn destroy_publisher_trampoline<R: RustBackend>(
    publisher: *mut NrosRmwPublisher,
) {
    let _ = unsafe { take_box::<R::Publisher>(publisher_data_mut(publisher)) };
}

unsafe extern "C" fn publish_raw_trampoline<R: RustBackend>(
    publisher: *mut NrosRmwPublisher,
    data: *const u8,
    len: usize,
) -> NrosRmwRet {
    let Some(p) = (unsafe { publisher_ref::<R::Publisher>(publisher) }) else {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    };
    if data.is_null() && len != 0 {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    let slice = unsafe { core::slice::from_raw_parts(data, len) };
    match Publisher::publish_raw(p, slice) {
        Ok(()) => NROS_RMW_RET_OK,
        Err(e) => ret_from_error(&e),
    }
}

// ============================================================================
// Trampolines — subscriber
// ============================================================================

unsafe extern "C" fn create_subscriber_trampoline<R: RustBackend>(
    session: *mut NrosRmwSession,
    topic_name: *const u8,
    type_name: *const u8,
    type_hash: *const u8,
    domain_id: u32,
    qos: *const NrosRmwQos,
    out: *mut NrosRmwSubscriber,
) -> NrosRmwRet {
    if out.is_null() || qos.is_null() {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    let Some(s) = (unsafe { session_mut::<R::Session>(session) }) else {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    };
    let node_name = unsafe { session_node_name(session) };
    let namespace = unsafe { session_namespace(session) };
    let topic = TopicInfo {
        name: unsafe { cstr_to_str(topic_name) },
        type_name: unsafe { cstr_to_str(type_name) },
        type_hash: unsafe { cstr_to_str(type_hash) },
        domain_id,
        node_name,
        namespace,
    };
    let qos_settings = qos_from_cffi(unsafe { &*qos });
    match Session::create_subscriber(s, &topic, qos_settings) {
        Ok(sub_handle) => {
            #[cfg(all(target_os = "none", not(feature = "std")))]
            {
                let Some(ptr) = (unsafe { static_subscriber_storage::insert(sub_handle) }) else {
                    return crate::NROS_RMW_RET_BAD_ALLOC;
                };
                unsafe {
                    (*out).backend_data = ptr as *mut c_void;
                }
                NROS_RMW_RET_OK
            }
            #[cfg(not(all(target_os = "none", not(feature = "std"))))]
            {
                let boxed = Box::into_raw(Box::new(sub_handle));
                unsafe {
                    (*out).backend_data = boxed as *mut c_void;
                }
                NROS_RMW_RET_OK
            }
        }
        Err(e) => ret_from_error(&e),
    }
}

unsafe extern "C" fn destroy_subscriber_trampoline<R: RustBackend>(
    subscriber: *mut NrosRmwSubscriber,
) {
    let slot = unsafe { subscriber_data_mut(subscriber) };
    #[cfg(all(target_os = "none", not(feature = "std")))]
    {
        if unsafe { static_subscriber_storage::take::<R::Subscriber>(*slot as *mut R::Subscriber) }
        {
            *slot = core::ptr::null_mut();
            return;
        }
    }
    let _ = unsafe { take_box::<R::Subscriber>(slot) };
}

unsafe extern "C" fn try_recv_raw_trampoline<R: RustBackend>(
    subscriber: *mut NrosRmwSubscriber,
    buf: *mut u8,
    buf_len: usize,
) -> i32 {
    let Some(s) = (unsafe { subscriber_mut::<R::Subscriber>(subscriber) }) else {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    };
    if buf.is_null() && buf_len != 0 {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    let slice = unsafe { core::slice::from_raw_parts_mut(buf, buf_len) };
    let key = unsafe { (*subscriber).backend_data as usize };

    #[cfg(feature = "safety-e2e")]
    if crate::take_cffi_integrity_request(key) {
        return match Subscriber::try_recv_validated(s, slice) {
            Ok(Some((n, status))) => {
                crate::store_cffi_integrity_status(key, status);
                n as i32
            }
            Ok(None) => NROS_RMW_RET_NO_DATA,
            Err(e) => ret_from_error(&e),
        };
    }

    match Subscriber::try_recv_raw_with_info(s, slice) {
        Ok(Some((n, info))) => {
            crate::store_cffi_message_info(key, info);
            n as i32
        }
        Ok(None) => NROS_RMW_RET_NO_DATA,
        Err(e) => ret_from_error(&e),
    }
}

unsafe extern "C" fn has_data_trampoline<R: RustBackend>(
    subscriber: *mut NrosRmwSubscriber,
) -> i32 {
    let Some(s) = (unsafe { subscriber_ref::<R::Subscriber>(subscriber) }) else {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    };
    if Subscriber::has_data(s) { 1 } else { 0 }
}

// Phase 231 (RFC-0038) — in-place subscription take across the C ABI.

unsafe extern "C" fn subscriber_supports_in_place_trampoline<R: RustBackend>(
    subscriber: *mut NrosRmwSubscriber,
) -> i32 {
    let Some(s) = (unsafe { subscriber_ref::<R::Subscriber>(subscriber) }) else {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    };
    if Subscriber::supports_process_in_place(s) {
        1
    } else {
        0
    }
}

unsafe extern "C" fn process_raw_in_place_trampoline<R: RustBackend>(
    subscriber: *mut NrosRmwSubscriber,
    ctx: *mut core::ffi::c_void,
    cb: unsafe extern "C" fn(ctx: *mut core::ffi::c_void, ptr: *const u8, len: usize),
) -> i32 {
    let Some(s) = (unsafe { subscriber_mut::<R::Subscriber>(subscriber) }) else {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    };
    match Subscriber::process_raw_in_place(s, |raw| unsafe { cb(ctx, raw.as_ptr(), raw.len()) }) {
        Ok(true) => 1,
        Ok(false) => NROS_RMW_RET_NO_DATA,
        Err(e) => ret_from_error(&e),
    }
}

// ============================================================================
// Trampolines — service server
// ============================================================================

unsafe extern "C" fn create_service_server_trampoline<R: RustBackend>(
    session: *mut NrosRmwSession,
    service_name: *const u8,
    type_name: *const u8,
    type_hash: *const u8,
    domain_id: u32,
    qos: *const NrosRmwQos,
    out: *mut NrosRmwServiceServer,
) -> NrosRmwRet {
    if out.is_null() || qos.is_null() {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    let Some(s) = (unsafe { session_mut::<R::Session>(session) }) else {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    };
    let node_name = unsafe { session_node_name(session) };
    let namespace = unsafe { session_namespace(session) };
    let info = nros_rmw::ServiceInfo {
        name: unsafe { cstr_to_str(service_name) },
        type_name: unsafe { cstr_to_str(type_name) },
        type_hash: unsafe { cstr_to_str(type_hash) },
        domain_id,
        node_name,
        namespace,
    };
    let qos_settings = qos_from_cffi(unsafe { &*qos });
    match Session::create_service_server(s, &info, qos_settings) {
        Ok(server) => {
            let boxed = Box::into_raw(Box::new(server));
            unsafe {
                (*out).backend_data = boxed as *mut c_void;
            }
            NROS_RMW_RET_OK
        }
        Err(e) => ret_from_error(&e),
    }
}

unsafe extern "C" fn destroy_service_server_trampoline<R: RustBackend>(
    server: *mut NrosRmwServiceServer,
) {
    let _ = unsafe { take_box::<R::ServiceServer>(service_server_data_mut(server)) };
}

unsafe extern "C" fn try_recv_request_trampoline<R: RustBackend>(
    server: *mut NrosRmwServiceServer,
    buf: *mut u8,
    buf_len: usize,
    seq_out: *mut i64,
) -> i32 {
    let Some(s) = (unsafe { service_server_mut::<R::ServiceServer>(server) }) else {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    };
    if (buf.is_null() && buf_len != 0) || seq_out.is_null() {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    let slice = unsafe { core::slice::from_raw_parts_mut(buf, buf_len) };
    let buf_start = slice.as_ptr() as usize;
    match ServiceServerTrait::try_recv_request(s, slice) {
        Ok(Some(req)) => {
            // The handle's `data` slice borrows from `buf`. Use its
            // offset within the caller's buffer to compute the payload
            // length; some backends prepend an envelope header so the
            // payload doesn't start at offset 0.
            let offset = (req.data.as_ptr() as usize).saturating_sub(buf_start);
            let len = req.data.len();
            unsafe {
                *seq_out = req.sequence_number;
            }
            // Move the payload to the start of the buffer so the C
            // caller sees a `(buf, len)` pair starting at offset 0,
            // matching the cyclonedds backend's contract.
            if offset != 0 {
                let total = offset + len;
                // SAFETY: `slice[..total]` is initialised by the
                // backend; copy_within respects overlapping ranges.
                slice.copy_within(offset..total, 0);
            }
            len as i32
        }
        Ok(None) => NROS_RMW_RET_NO_DATA,
        Err(e) => ret_from_error(&e),
    }
}

unsafe extern "C" fn has_request_trampoline<R: RustBackend>(
    server: *mut NrosRmwServiceServer,
) -> i32 {
    let Some(s) = (unsafe { service_server_ref::<R::ServiceServer>(server) }) else {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    };
    if ServiceServerTrait::has_request(s) {
        1
    } else {
        0
    }
}

unsafe extern "C" fn send_reply_trampoline<R: RustBackend>(
    server: *mut NrosRmwServiceServer,
    seq: i64,
    data: *const u8,
    len: usize,
) -> NrosRmwRet {
    let Some(s) = (unsafe { service_server_mut::<R::ServiceServer>(server) }) else {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    };
    if data.is_null() && len != 0 {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    let slice = unsafe { core::slice::from_raw_parts(data, len) };
    match ServiceServerTrait::send_reply(s, seq, slice) {
        Ok(()) => NROS_RMW_RET_OK,
        Err(e) => ret_from_error(&e),
    }
}

// ============================================================================
// Trampolines — service client
// ============================================================================

unsafe extern "C" fn create_service_client_trampoline<R: RustBackend>(
    session: *mut NrosRmwSession,
    service_name: *const u8,
    type_name: *const u8,
    type_hash: *const u8,
    domain_id: u32,
    qos: *const NrosRmwQos,
    out: *mut NrosRmwServiceClient,
) -> NrosRmwRet {
    if out.is_null() || qos.is_null() {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    let Some(s) = (unsafe { session_mut::<R::Session>(session) }) else {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    };
    let node_name = unsafe { session_node_name(session) };
    let namespace = unsafe { session_namespace(session) };
    let info = nros_rmw::ServiceInfo {
        name: unsafe { cstr_to_str(service_name) },
        type_name: unsafe { cstr_to_str(type_name) },
        type_hash: unsafe { cstr_to_str(type_hash) },
        domain_id,
        node_name,
        namespace,
    };
    let qos_settings = qos_from_cffi(unsafe { &*qos });
    match Session::create_service_client(s, &info, qos_settings) {
        Ok(client) => {
            let boxed = Box::into_raw(Box::new(client));
            unsafe {
                (*out).backend_data = boxed as *mut c_void;
            }
            NROS_RMW_RET_OK
        }
        Err(e) => ret_from_error(&e),
    }
}

unsafe extern "C" fn destroy_service_client_trampoline<R: RustBackend>(
    client: *mut NrosRmwServiceClient,
) {
    let _ = unsafe { take_box::<R::ServiceClient>(service_client_data_mut(client)) };
}

unsafe extern "C" fn call_raw_trampoline<R: RustBackend>(
    client: *mut NrosRmwServiceClient,
    request: *const u8,
    req_len: usize,
    reply_buf: *mut u8,
    reply_buf_len: usize,
) -> i32 {
    let Some(c) = (unsafe { service_client_mut::<R::ServiceClient>(client) }) else {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    };
    if (request.is_null() && req_len != 0) || (reply_buf.is_null() && reply_buf_len != 0) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    let req = unsafe { core::slice::from_raw_parts(request, req_len) };
    let reply = unsafe { core::slice::from_raw_parts_mut(reply_buf, reply_buf_len) };
    // `call_raw` is marked deprecated on the trait (the executor-driven
    // `Client::call` → `Promise::wait` path is the supported flow), but
    // the cffi vtable still exposes a blocking call entry point for C
    // consumers that don't have an executor handle. The deprecation is
    // about API ergonomics, not correctness, so the adapter forwards
    // through unchanged.
    #[allow(deprecated)]
    match ServiceClientTrait::call_raw(c, req, reply) {
        Ok(n) => n as i32,
        Err(e) => ret_from_error(&e),
    }
}

// Phase 130.8 — non-blocking send/recv trampolines. Forwards to the
// backend's `ServiceClientTrait::send_request_raw` /
// `try_recv_reply_raw` so Rust-backed cffi consumers (dust-DDS,
// future Rust Cyclone wrapper) skip the legacy blocking call_raw
// fallback inside `CffiServiceClient`.
unsafe extern "C" fn send_request_raw_trampoline<R: RustBackend>(
    client: *mut NrosRmwServiceClient,
    request: *const u8,
    req_len: usize,
) -> NrosRmwRet {
    let Some(c) = (unsafe { service_client_mut::<R::ServiceClient>(client) }) else {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    };
    if request.is_null() && req_len != 0 {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    let req = unsafe { core::slice::from_raw_parts(request, req_len) };
    match ServiceClientTrait::send_request_raw(c, req) {
        Ok(()) => NROS_RMW_RET_OK,
        Err(e) => ret_from_error(&e),
    }
}

unsafe extern "C" fn try_recv_reply_raw_trampoline<R: RustBackend>(
    client: *mut NrosRmwServiceClient,
    reply_buf: *mut u8,
    reply_buf_len: usize,
) -> i32 {
    let Some(c) = (unsafe { service_client_mut::<R::ServiceClient>(client) }) else {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    };
    if reply_buf.is_null() && reply_buf_len != 0 {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    let reply = unsafe { core::slice::from_raw_parts_mut(reply_buf, reply_buf_len) };
    match ServiceClientTrait::try_recv_reply_raw(c, reply) {
        Ok(Some(n)) => n as i32,
        Ok(None) => NROS_RMW_RET_NO_DATA,
        Err(e) => ret_from_error(&e),
    }
}

// ============================================================================
// Event-callback bridge (Phase 115.L.0.events)
// ============================================================================
//
// Events / liveliness / deadline are wired through the vtable (see the
// `register_subscriber_event` / `register_publisher_event` /
// `assert_publisher_liveliness` / `next_deadline_ms` trampolines registered
// above) — the stale "TODO: wire through" header was removed (Phase 192.9).
//
// `NrosRmwEventCallback` (cffi shape) and `nros_rmw::EventCallback`
// (trait shape) have *layout-identical* arguments:
//
//   * NrosRmwEventKind  ↔ EventKind     — both `#[repr(u8)]`, same variant ids.
//   * *const NrosRmwEventPayload ↔ *const c_void  — fn-ptr-width pointer; the
//     trait callback dereferences as `*const LivelinessChangedStatus` /
//     `*const CountStatus`, the cffi callback dereferences as the matching
//     union member. The field-by-field layout matches today (see the
//     `_event_payload_layout_match` const-asserts below).
//   * *mut c_void       ↔ *mut c_void   — identical.
//
// Therefore the cffi callback can be transmuted into a trait callback
// pointer; the receiving Rust trait code calls it via the trait
// signature, but the bytes on the wire are the cffi struct.

const _: () = {
    use core::mem::{align_of, size_of};
    assert!(
        size_of::<crate::NrosRmwLivelinessChangedStatus>()
            == size_of::<nros_rmw::LivelinessChangedStatus>()
    );
    assert!(
        align_of::<crate::NrosRmwLivelinessChangedStatus>()
            == align_of::<nros_rmw::LivelinessChangedStatus>()
    );
    assert!(size_of::<crate::NrosRmwCountStatus>() == size_of::<nros_rmw::CountStatus>());
    assert!(align_of::<crate::NrosRmwCountStatus>() == align_of::<nros_rmw::CountStatus>());
    // EventKind enum tags must round-trip 0..=4 between the two
    // `#[repr(u8)]` definitions.
    assert!(
        NrosRmwEventKind::LivelinessChanged as u8 == nros_rmw::EventKind::LivelinessChanged as u8
    );
    assert!(
        NrosRmwEventKind::RequestedDeadlineMissed as u8
            == nros_rmw::EventKind::RequestedDeadlineMissed as u8
    );
    assert!(NrosRmwEventKind::MessageLost as u8 == nros_rmw::EventKind::MessageLost as u8);
    assert!(NrosRmwEventKind::LivelinessLost as u8 == nros_rmw::EventKind::LivelinessLost as u8);
    assert!(
        NrosRmwEventKind::OfferedDeadlineMissed as u8
            == nros_rmw::EventKind::OfferedDeadlineMissed as u8
    );
};

unsafe extern "C" fn register_subscriber_event_trampoline<R: RustBackend>(
    subscriber: *mut NrosRmwSubscriber,
    kind: NrosRmwEventKind,
    deadline_ms: u32,
    cb: NrosRmwEventCallback,
    user_context: *mut c_void,
) -> NrosRmwRet {
    let Some(s) = (unsafe { subscriber_mut::<R::Subscriber>(subscriber) }) else {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    };
    // SAFETY: cffi `NrosRmwEventKind` and trait `EventKind` are both
    // `#[repr(u8)]` with identical variant numbering (asserted at
    // compile time above).
    let trait_kind: nros_rmw::EventKind = unsafe { core::mem::transmute(kind) };
    // SAFETY: see module-level note. `cb`'s parameter types
    // `(NrosRmwEventKind, *const NrosRmwEventPayload, *mut c_void)` are
    // ABI-compatible with the trait's `(EventKind, *const c_void, *mut c_void)`.
    let trait_cb: nros_rmw::EventCallback = unsafe { core::mem::transmute(cb) };
    let res = unsafe {
        Subscriber::register_event_callback(s, trait_kind, deadline_ms, trait_cb, user_context)
    };
    match res {
        Ok(()) => NROS_RMW_RET_OK,
        Err(e) => {
            // Backend's `Unsupported` mapping for events is its
            // `serialization_error()` by trait-doc default; the cffi
            // contract for "this event kind unsupported" is its own
            // ret code, so map any error here to UNSUPPORTED rather
            // than e.g. SERIALIZATION_ERROR — the caller's mental
            // model is "vtable said no events," not "marshalling
            // failed."
            let _ = e;
            NROS_RMW_RET_UNSUPPORTED
        }
    }
}

unsafe extern "C" fn register_publisher_event_trampoline<R: RustBackend>(
    publisher: *mut NrosRmwPublisher,
    kind: NrosRmwEventKind,
    deadline_ms: u32,
    cb: NrosRmwEventCallback,
    user_context: *mut c_void,
) -> NrosRmwRet {
    // Publisher::register_event_callback takes `&mut self`. Need a
    // mut-ptr to the boxed handle.
    if publisher.is_null() {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    let p_ptr = unsafe { (*publisher).backend_data } as *mut R::Publisher;
    if p_ptr.is_null() {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    let p = unsafe { &mut *p_ptr };
    let trait_kind: nros_rmw::EventKind = unsafe { core::mem::transmute(kind) };
    let trait_cb: nros_rmw::EventCallback = unsafe { core::mem::transmute(cb) };
    let res = unsafe {
        Publisher::register_event_callback(p, trait_kind, deadline_ms, trait_cb, user_context)
    };
    match res {
        Ok(()) => NROS_RMW_RET_OK,
        Err(_) => NROS_RMW_RET_UNSUPPORTED,
    }
}

unsafe extern "C" fn assert_publisher_liveliness_trampoline<R: RustBackend>(
    publisher: *mut NrosRmwPublisher,
) -> NrosRmwRet {
    let Some(p) = (unsafe { publisher_ref::<R::Publisher>(publisher) }) else {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    };
    match Publisher::assert_liveliness(p) {
        Ok(()) => NROS_RMW_RET_OK,
        Err(e) => ret_from_error(&e),
    }
}

unsafe extern "C" fn next_deadline_ms_trampoline<R: RustBackend>(
    session: *const NrosRmwSession,
) -> i32 {
    let Some(s) = (unsafe { session_ref::<R::Session>(session) }) else {
        return -1;
    };
    Session::next_deadline_ms(s).map(|v| v as i32).unwrap_or(-1)
}

unsafe extern "C" fn set_wake_callback_trampoline<R: RustBackend>(
    session: *mut NrosRmwSession,
    cb: Option<unsafe extern "C" fn(ctx: *mut core::ffi::c_void)>,
    ctx: *mut core::ffi::c_void,
) -> NrosRmwRet {
    let Some(s) = (unsafe { session_mut::<R::Session>(session) }) else {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    };
    // Phase 124.B.1 — delegate to the Rust backend. Default trait
    // body ignores; concrete backends opt in.
    // SAFETY: this trampoline forwards the C ABI's callback/context
    // lifetime contract to the Rust backend.
    unsafe { Session::set_wake_callback(s, cb, ctx) };
    NROS_RMW_RET_OK
}

unsafe extern "C" fn service_server_available_trampoline<R: RustBackend>(
    client: *mut NrosRmwServiceClient,
) -> i32 {
    // Phase 124.C.1 — delegate to the Rust backend's
    // `ServiceClientTrait::server_available` impl. Default trait
    // body returns `Err(TransportError::Unsupported)`; concrete
    // backends opt in by overriding.
    let Some(c) = (unsafe { service_client_mut::<R::ServiceClient>(client) }) else {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    };
    match ServiceClientTrait::server_available(c) {
        Ok(true) => 1,
        Ok(false) => 0,
        Err(e) => ret_from_error(&e),
    }
}

unsafe extern "C" fn ping_session_trampoline<R: RustBackend>(
    session: *mut NrosRmwSession,
    timeout_ms: i32,
) -> NrosRmwRet {
    // Phase 124.F.1 — delegate to the Rust backend's
    // `Session::ping_session` impl. Default trait body returns
    // `Err(TransportError::Unsupported)`; concrete backends opt in
    // by overriding.
    let Some(s) = (unsafe { session_mut::<R::Session>(session) }) else {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    };
    match Session::ping_session(s, timeout_ms) {
        Ok(()) => NROS_RMW_RET_OK,
        Err(e) => ret_from_error(&e),
    }
}

unsafe extern "C" fn publish_streamed_trampoline<R: RustBackend>(
    publisher: *mut NrosRmwPublisher,
    size_cb: unsafe extern "C" fn(out_total_len: *mut usize, user_ctx: *mut c_void),
    chunk_cb: unsafe extern "C" fn(
        out_buf: *mut u8,
        cap: usize,
        out_written: *mut usize,
        user_ctx: *mut c_void,
    ),
    user_ctx: *mut c_void,
) -> NrosRmwRet {
    // Phase 124.E.1 — delegate to the Rust backend's
    // `Publisher::publish_streamed` impl. Default trait body fires
    // the staging-buffer fallback (124.E.2); concrete backends opt
    // in by overriding for true streamed publish into the network
    // buffer.
    let Some(p) = (unsafe { publisher_ref::<R::Publisher>(publisher) }) else {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    };
    // SAFETY: this trampoline is entered from the C ABI with the same
    // callback/user_ctx lifetime contract required by `Publisher::publish_streamed`.
    match unsafe { Publisher::publish_streamed(p, size_cb, chunk_cb, user_ctx) } {
        Ok(()) => NROS_RMW_RET_OK,
        Err(e) => ret_from_error(&e),
    }
}

unsafe extern "C" fn try_recv_sequence_trampoline<R: RustBackend>(
    subscriber: *mut NrosRmwSubscriber,
    buf: *mut u8,
    per_msg_cap: usize,
    max_msgs: usize,
    out_lens: *mut usize,
) -> i32 {
    // Phase 124.D.1 — delegate to the Rust backend's
    // `Subscriber::try_recv_sequence` impl. Default trait body
    // loop-drives `try_recv_raw`; concrete backends opt in by
    // overriding for a native batch take.
    let Some(s) = (unsafe { subscriber_mut::<R::Subscriber>(subscriber) }) else {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    };
    if buf.is_null() || out_lens.is_null() || per_msg_cap == 0 {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    // SAFETY: caller pinky-promised a contiguous block of
    // `max_msgs * per_msg_cap` bytes at `buf` and at least
    // `max_msgs` `usize` slots at `out_lens`. The buffer is
    // exclusively borrowed for the duration of this call.
    let buf_slice =
        unsafe { core::slice::from_raw_parts_mut(buf, max_msgs.saturating_mul(per_msg_cap)) };
    let lens_slice = unsafe { core::slice::from_raw_parts_mut(out_lens, max_msgs) };
    match Subscriber::try_recv_sequence(s, buf_slice, per_msg_cap, max_msgs, lens_slice) {
        Ok(count) => count as i32,
        Err(e) => ret_from_error(&e),
    }
}

// ============================================================================
// Pointer plumbing — keep one place where each entity's
// `backend_data` is dereferenced.
// ============================================================================
//
// Validity contract:
//
//   - The C-side wrappers (`CffiSession`, `CffiPublisher`, …) hand
//     each trampoline an entity-struct pointer whose `backend_data`
//     was last written by *this* adapter's create-trampoline. That
//     write is `Box::into_raw(Box::new(handle))` of `R::Session` /
//     `R::Publisher` / … so the pointer aliases a valid Box<T>.
//   - `_mut` variants give `&mut T`; the runtime serialises access
//     per the executor's single-thread invariant.
//   - `take_box` reclaims ownership; subsequent calls see a null
//     `backend_data` and return `INVALID_ARGUMENT`.

#[inline]
unsafe fn session_data_mut(session: *mut NrosRmwSession) -> &'static mut *mut c_void {
    unsafe { &mut (*session).backend_data }
}

#[inline]
unsafe fn publisher_data_mut(publisher: *mut NrosRmwPublisher) -> &'static mut *mut c_void {
    unsafe { &mut (*publisher).backend_data }
}

#[inline]
unsafe fn subscriber_data_mut(subscriber: *mut NrosRmwSubscriber) -> &'static mut *mut c_void {
    unsafe { &mut (*subscriber).backend_data }
}

#[inline]
unsafe fn service_server_data_mut(server: *mut NrosRmwServiceServer) -> &'static mut *mut c_void {
    unsafe { &mut (*server).backend_data }
}

#[inline]
unsafe fn service_client_data_mut(client: *mut NrosRmwServiceClient) -> &'static mut *mut c_void {
    unsafe { &mut (*client).backend_data }
}

#[inline]
unsafe fn session_mut<'a, T>(session: *mut NrosRmwSession) -> Option<&'a mut T> {
    if session.is_null() {
        return None;
    }
    let p = unsafe { (*session).backend_data } as *mut T;
    if p.is_null() {
        None
    } else {
        Some(unsafe { &mut *p })
    }
}

#[inline]
unsafe fn session_ref<'a, T>(session: *const NrosRmwSession) -> Option<&'a T> {
    if session.is_null() {
        return None;
    }
    let p = unsafe { (*session).backend_data } as *const T;
    if p.is_null() {
        None
    } else {
        Some(unsafe { &*p })
    }
}

#[inline]
unsafe fn publisher_ref<'a, T>(publisher: *mut NrosRmwPublisher) -> Option<&'a T> {
    if publisher.is_null() {
        return None;
    }
    let p = unsafe { (*publisher).backend_data } as *const T;
    if p.is_null() {
        None
    } else {
        Some(unsafe { &*p })
    }
}

#[inline]
unsafe fn subscriber_mut<'a, T>(subscriber: *mut NrosRmwSubscriber) -> Option<&'a mut T> {
    if subscriber.is_null() {
        return None;
    }
    let p = unsafe { (*subscriber).backend_data } as *mut T;
    if p.is_null() {
        None
    } else {
        Some(unsafe { &mut *p })
    }
}

#[inline]
unsafe fn subscriber_ref<'a, T>(subscriber: *mut NrosRmwSubscriber) -> Option<&'a T> {
    if subscriber.is_null() {
        return None;
    }
    let p = unsafe { (*subscriber).backend_data } as *const T;
    if p.is_null() {
        None
    } else {
        Some(unsafe { &*p })
    }
}

#[inline]
unsafe fn service_server_mut<'a, T>(server: *mut NrosRmwServiceServer) -> Option<&'a mut T> {
    if server.is_null() {
        return None;
    }
    let p = unsafe { (*server).backend_data } as *mut T;
    if p.is_null() {
        None
    } else {
        Some(unsafe { &mut *p })
    }
}

#[inline]
unsafe fn service_server_ref<'a, T>(server: *mut NrosRmwServiceServer) -> Option<&'a T> {
    if server.is_null() {
        return None;
    }
    let p = unsafe { (*server).backend_data } as *const T;
    if p.is_null() {
        None
    } else {
        Some(unsafe { &*p })
    }
}

#[inline]
unsafe fn service_client_mut<'a, T>(client: *mut NrosRmwServiceClient) -> Option<&'a mut T> {
    if client.is_null() {
        return None;
    }
    let p = unsafe { (*client).backend_data } as *mut T;
    if p.is_null() {
        None
    } else {
        Some(unsafe { &mut *p })
    }
}

#[inline]
unsafe fn take_box<T>(slot: &mut *mut c_void) -> Option<Box<T>> {
    if slot.is_null() {
        return None;
    }
    let ptr = *slot as *mut T;
    *slot = core::ptr::null_mut();
    if ptr.is_null() {
        None
    } else {
        // SAFETY: this pointer was minted by `Box::into_raw` in the
        // corresponding create-trampoline; we are taking ownership
        // back and the slot has been cleared so no second take is
        // possible.
        Some(unsafe { Box::from_raw(ptr) })
    }
}
