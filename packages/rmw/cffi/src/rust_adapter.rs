//! Phase 115.L.0 — Generic Rust-trait → C-vtable adapter.
//!
//! `RustBackendAdapter<R>` converts any `R: nros_rmw::Rmw` whose
//! associated types implement the matching `Session` / `Publisher` /
//! `Subscription` / `ServiceTrait` / `ClientTrait` traits
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
//! - Publisher / Subscription / Service / Client: same
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
use core::{
    ffi::{c_char, c_void},
    marker::PhantomData,
};

use nros_rmw::{
    ClientTrait, Publisher, QoSProfile, Rmw, RmwConfig, ServiceTrait, Session, SessionMode,
    Subscription, TopicInfo, TransportError,
};

use crate::{
    EMPTY_VTABLE, MAX_SESSION_PROPERTIES, NROS_RMW_RET_INVALID_ARGUMENT, NROS_RMW_RET_OK,
    NROS_RMW_RET_UNSUPPORTED, NrosRmwClient, NrosRmwEventCallback, NrosRmwEventKind, NrosRmwNode,
    NrosRmwPublisher, NrosRmwQos, NrosRmwRet, NrosRmwService, NrosRmwSession,
    NrosRmwSessionOptions, NrosRmwSubscription, NrosRmwVtable, event_kind_from_c, ret_from_error,
    rmw_publisher_options_t, rmw_subscription_options_t,
};
// phase-381 W3 — the endpoint-info slots marshal the generated ABI types
// directly. Imported from `generated` rather than re-exported through the crate
// root: these are bindgen output and the root deliberately re-exports only the
// hand-maintained surface.
use crate::generated::{
    NROS_RMW_DURABILITY_UNKNOWN, NROS_RMW_HISTORY_UNKNOWN, NROS_RMW_RELIABILITY_UNKNOWN,
    rmw_endpoint_type_t, rmw_gid_t, rmw_liveliness_kind_t, rmw_qos_profile_t,
    rmw_topic_endpoint_info_t,
};

#[cfg(all(target_os = "none", not(feature = "std")))]
mod static_subscriber_storage {
    use core::{cell::UnsafeCell, mem, ptr};

    use portable_atomic::{AtomicBool, Ordering};

    // Issue 0269 — the pool size is build-time configurable via
    // `NROS_RMW_SUBSCRIBER_SLOTS` (build.rs, default 8). The old
    // hardcoded 4 silently capped every embedded session at four
    // subscriptions: the fifth `create_subscription` hit the exhausted
    // pool, returned BAD_ALLOC, and the executor surfaced it as an
    // opaque `SubscriberCreationFailed` (the autoware_sentinel comp-all
    // wall — every std target Boxes instead and never sees a limit).
    const SLOT_COUNT: usize = crate::parse_env_usize(
        env!("NROS_RMW_SUBSCRIBER_SLOTS"),
        "NROS_RMW_SUBSCRIBER_SLOTS must be a decimal integer",
    );
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

    // issue 0739 — declare the arithmetic so the pool inventory can price it.
    // 0271 measured this pool at 8,192 bytes on an image that had never heard of
    // the knob; `SLOT_SIZE` is the literal above, so the figure is derivable.
    // nros-pool: SLOTS = NROS_RMW_SUBSCRIBER_SLOTS * 1024
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
            SubscriptionHandle = Self::Subscription,
            ServiceHandle = Self::Service,
            ClientHandle = Self::Client,
        > + 'static;
    type Publisher: Publisher<Error = TransportError> + 'static;
    type Subscription: Subscription<Error = TransportError> + 'static;
    type Service: ServiceTrait<Error = TransportError> + 'static;
    type Client: ClientTrait<Error = TransportError> + 'static;

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
    <R::Session as Session>::SubscriptionHandle: Subscription<Error = TransportError> + 'static,
    <R::Session as Session>::ServiceHandle: ServiceTrait<Error = TransportError> + 'static,
    <R::Session as Session>::ClientHandle: ClientTrait<Error = TransportError> + 'static,
{
    type Session = R::Session;
    type Publisher = <R::Session as Session>::PublisherHandle;
    type Subscription = <R::Session as Session>::SubscriptionHandle;
    type Service = <R::Session as Session>::ServiceHandle;
    type Client = <R::Session as Session>::ClientHandle;

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
unsafe fn cstr_to_str<'a>(ptr: *const core::ffi::c_char) -> &'a str {
    if ptr.is_null() {
        return "";
    }
    let ptr = ptr.cast::<u8>();
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

/// Borrow a NUL-terminated C string as `&str`, distinguishing "absent" and
/// "not text" from "empty" — which [`cstr_to_str`] deliberately does not.
///
/// `cstr_to_str` collapses NULL and invalid UTF-8 into `""` because its
/// callers (node names, topic names) treat an unusable name as an absent one.
/// A configuration property cannot: an empty key is not a key, and a value the
/// backend cannot read must be reported, not passed on as `""`.
unsafe fn cstr_to_str_strict<'a>(ptr: *const core::ffi::c_char) -> Option<&'a str> {
    if ptr.is_null() {
        return None;
    }
    let bytes = ptr.cast::<u8>();
    let mut len = 0usize;
    while len < 4096 && unsafe { *bytes.add(len) } != 0 {
        len += 1;
    }
    if len == 0 {
        return None;
    }
    let slice = unsafe { core::slice::from_raw_parts(bytes, len) };
    core::str::from_utf8(slice).ok()
}

/// Copy `options->properties` into `out`, returning how many entries were
/// written, or the `NrosRmwRet` to fail the session with.
///
/// # Safety
/// `options`, when non-NULL, must point at a valid `rmw_session_options_t`
/// whose `properties` array holds `property_count` valid entries.
unsafe fn collect_session_properties<'a>(
    options: *const NrosRmwSessionOptions,
    out: &mut [(&'a str, &'a str); MAX_SESSION_PROPERTIES],
) -> Result<usize, NrosRmwRet> {
    if options.is_null() {
        return Ok(0);
    }
    let opts = unsafe { &*options };
    let count = opts.property_count;
    if count == 0 {
        return Ok(0);
    }
    if opts.properties.is_null() || count > MAX_SESSION_PROPERTIES {
        return Err(NROS_RMW_RET_INVALID_ARGUMENT);
    }
    for (i, slot) in out.iter_mut().enumerate().take(count) {
        let entry = unsafe { &*opts.properties.add(i) };
        let (Some(key), Some(value)) = (unsafe { cstr_to_str_strict(entry.key) }, unsafe {
            cstr_to_str_strict(entry.value)
        }) else {
            return Err(NROS_RMW_RET_INVALID_ARGUMENT);
        };
        *slot = (key, value);
    }
    Ok(count)
}

/// Convert the cffi QoS view back into a `nros_rmw::QoSProfile`. The
/// adapter trampolines call this when forwarding `create_publisher` /
/// `create_subscription` into the Rust trait.
fn qos_from_cffi(q: &NrosRmwQos) -> QoSProfile {
    use crate::generated;
    use nros_rmw::{
        QoSDurabilityPolicy, QoSHistoryPolicy, QoSLivelinessPolicy, QoSReliabilityPolicy,
    };
    // Phase 376 W5/B2 — these were `== 0` tests against a dense 0/1 encoding.
    // Under upstream's numbering 0 is SYSTEM_DEFAULT for every policy, so each
    // test now has three cases, not two. Written as a match on the named
    // constants so an unrecognised value is visible rather than folded into
    // one arm.
    //
    // issue 0829 — SYSTEM_DEFAULT now RAISES to the sentinel variant instead
    // of being folded to the ROS default here. This function is the inbound
    // edge of a Rust backend, and resolving an absence is the BACKEND's job:
    // folding at the edge made every Rust backend answer the sentinel
    // identically, which is exactly what upstream does not do. UNKNOWN keeps
    // its old fold — it is a read-back artefact ("the backend could not
    // determine this"), not a request, so it has no sentinel meaning to carry.
    QoSProfile {
        reliability: match q.reliability as i32 {
            generated::NROS_RMW_RELIABILITY_SYSTEM_DEFAULT => QoSReliabilityPolicy::SystemDefault,
            generated::NROS_RMW_RELIABILITY_BEST_EFFORT => QoSReliabilityPolicy::BestEffort,
            generated::NROS_RMW_RELIABILITY_RELIABLE => QoSReliabilityPolicy::Reliable,
            _ => QoSReliabilityPolicy::Reliable,
        },
        durability: match q.durability as i32 {
            generated::NROS_RMW_DURABILITY_SYSTEM_DEFAULT => QoSDurabilityPolicy::SystemDefault,
            generated::NROS_RMW_DURABILITY_TRANSIENT_LOCAL => QoSDurabilityPolicy::TransientLocal,
            _ => QoSDurabilityPolicy::Volatile,
        },
        history: match q.history as i32 {
            generated::NROS_RMW_HISTORY_SYSTEM_DEFAULT => QoSHistoryPolicy::SystemDefault,
            generated::NROS_RMW_HISTORY_KEEP_ALL => QoSHistoryPolicy::KeepAll,
            _ => QoSHistoryPolicy::KeepLast,
        },
        // The depth sentinel is a VALUE (0), so it needs no raising — it
        // already reads as `DEPTH_SYSTEM_DEFAULT` on the far side.
        depth: q.depth as u32,
        // phase-301 (issue 0240): the express hint left the QoS struct; the
        // create trampolines thread it via `TopicInfo` from the options param.
        tx_express: false,
        deadline_ms: q.deadline_ms,
        lifespan_ms: q.lifespan_ms,
        liveliness_kind: match q.liveliness_kind as u32 {
            generated::rmw_liveliness_kind_t::NROS_RMW_LIVELINESS_AUTOMATIC => {
                QoSLivelinessPolicy::Automatic
            }
            generated::rmw_liveliness_kind_t::NROS_RMW_LIVELINESS_MANUAL_BY_NODE => {
                QoSLivelinessPolicy::ManualByNode
            }
            generated::rmw_liveliness_kind_t::NROS_RMW_LIVELINESS_MANUAL_BY_TOPIC => {
                QoSLivelinessPolicy::ManualByTopic
            }
            _ => QoSLivelinessPolicy::None,
        },
        liveliness_lease_ms: q.liveliness_lease_ms,
        avoid_ros_namespace_conventions: q.avoid_ros_namespace_conventions != 0,
    }
}

// Phase 376 W5/B1 — `session_node_name` and `session_namespace` are GONE with
// the `entity_view` fabrication they served. They read the OWNING NODE's
// identity off a session struct the shim rewrote per call, which is how node
// identity reached a backend before the node itself did.

/// The node's own identity.
unsafe fn node_name_of<'a>(node: *const NrosRmwNode) -> Option<&'a str> {
    let name = unsafe { cstr_to_str((*node).name) };
    if name.is_empty() { None } else { Some(name) }
}

unsafe fn node_namespace_of<'a>(node: *const NrosRmwNode) -> &'a str {
    let namespace = unsafe { cstr_to_str((*node).namespace_) };
    if namespace.is_empty() { "/" } else { namespace }
}

// ============================================================================
// Adapter
// ============================================================================

/// Longest serialization-format name this seam can hand across the C ABI.
///
/// RFC-0088 D2 — the vtable slot carries the format's cross-image identity
/// STRING, and a Rust backend spells that identity as a `&'static str`
/// (`Session::SERIALIZATION_FORMAT`), which is not NUL-terminated. The
/// conversion therefore has to happen somewhere, and it happens HERE, at
/// compile time, into per-`R` static storage — never at call time into a
/// buffer whose lifetime the slot's `const char *` contract cannot express.
///
/// 32 is generous for a name like `"cdr"`; a longer one is a const-eval error
/// rather than a truncation, because a truncated format name is a different,
/// plausible format.
const FORMAT_NAME_CAP: usize = 32;

/// NUL-terminate a format name at compile time.
const fn format_name_cstr(name: &str) -> [u8; FORMAT_NAME_CAP] {
    let bytes = name.as_bytes();
    assert!(
        bytes.len() < FORMAT_NAME_CAP,
        "serialization format name does not fit the vtable slot's buffer"
    );
    let mut out = [0u8; FORMAT_NAME_CAP];
    let mut i = 0;
    while i < bytes.len() {
        out[i] = bytes[i];
        i += 1;
    }
    out
}

/// `get_serialization_format` — RFC-0088 D4.
///
/// Takes no arguments, exactly as upstream's `rmw_get_serialization_format()`
/// does, because the answer is a property of the BACKEND and not of any entity
/// it created. What differs from upstream is who holds the question: upstream
/// has one middleware per process and can answer from a global, while
/// `nros_rmw_cffi_register_named` admits several backends in one image, so the
/// answer is per-vtable — which is per-session, since a session is opened
/// against one vtable.
unsafe extern "C" fn get_serialization_format_trampoline<R: RustBackend>() -> *const c_char {
    RustBackendAdapter::<R>::SERIALIZATION_FORMAT_CSTR
        .as_ptr()
        .cast()
}

/// Wraps a Rust `Rmw` backend behind the canonical
/// [`NrosRmwVtable`] C ABI. See module docs.
pub struct RustBackendAdapter<R>(PhantomData<R>);

impl<R: RustBackend> RustBackendAdapter<R> {
    /// RFC-0088 D4 — `R`'s serialization format, NUL-terminated, in per-`R`
    /// static storage so the `get_serialization_format` slot can hand out a
    /// `const char *` that outlives the call.
    ///
    /// `&<const expr>` in a const initialiser is promoted to `'static`, which
    /// is what makes the slot's lifetime contract true rather than merely
    /// hoped for.
    const SERIALIZATION_FORMAT_CSTR: &'static [u8; FORMAT_NAME_CAP] =
        &format_name_cstr(<R::Session as Session>::SERIALIZATION_FORMAT);

    /// Monomorphised vtable for backend `R`. The `const` is promoted
    /// to per-type static storage, so `&Self::VTABLE` has `'static`
    /// lifetime — safe to hand to `nros_rmw_cffi_register`.
    pub const VTABLE: NrosRmwVtable = NrosRmwVtable {
        create_session: Some(create_session_trampoline::<R>),
        destroy_session: Some(destroy_session_trampoline::<R>),
        drive_io: Some(drive_io_trampoline::<R>),
        create_publisher: Some(create_publisher_trampoline::<R>),
        destroy_publisher: Some(destroy_publisher_trampoline::<R>),
        publish: Some(publish_trampoline::<R>),
        create_subscription: Some(create_subscription_trampoline::<R>),
        destroy_subscription: Some(destroy_subscription_trampoline::<R>),
        take: Some(take_trampoline::<R>),
        has_data: Some(has_data_trampoline::<R>),
        create_service: Some(create_service_trampoline::<R>),
        destroy_service: Some(destroy_service_trampoline::<R>),
        take_request: Some(take_request_trampoline::<R>),
        has_request: Some(has_request_trampoline::<R>),
        send_response: Some(send_response_trampoline::<R>),
        create_client: Some(create_client_trampoline::<R>),
        destroy_client: Some(destroy_client_trampoline::<R>),
        // Phase-301 (issue 0240) — `send_request_raw` + `take_response_raw`
        // is the one request/reply path; the blocking `call_raw` slot is gone.
        send_request: Some(send_request_trampoline::<R>),
        take_response: Some(take_response_trampoline::<R>),
        subscription_event_init: Some(subscription_event_init_trampoline::<R>),
        publisher_event_init: Some(publisher_event_init_trampoline::<R>),
        publisher_assert_liveliness: Some(publisher_assert_liveliness_trampoline::<R>),
        next_deadline_ms: Some(next_deadline_ms_trampoline::<R>),
        set_wake_callback: Some(set_wake_callback_trampoline::<R>),
        // Phase 124.A — zero-copy slots default to NULL on the
        // generic adapter; per-backend opt-in via dedicated trampolines
        // (see `nros-rmw-zenoh` for the first implementation in 124.A.4).
        // Runtime falls back to the arena path when these are NULL.
        service_server_is_available: Some(service_server_is_available_trampoline::<R>),
        take_sequence: Some(take_sequence_trampoline::<R>),
        publish_streamed: Some(publish_streamed_trampoline::<R>),
        ping_session: Some(ping_session_trampoline::<R>),
        subscription_supports_in_place: Some(subscription_supports_in_place_trampoline::<R>),
        process_raw_in_place: Some(process_raw_in_place_trampoline::<R>),
        // phase-381 W3 — graph enumeration. The trait default is
        // `Unsupported`, so a backend with no graph (XRCE) answers that rather
        // than an empty one: "cannot tell you" and "nothing is there" are
        // different answers and W6 keeps them distinguishable.
        get_node_names: Some(get_node_names_trampoline::<R>),
        count_publishers: Some(count_publishers_trampoline::<R>),
        count_subscribers: Some(count_subscribers_trampoline::<R>),
        get_topic_names_and_types: Some(get_topic_names_and_types_trampoline::<R>),
        get_service_names_and_types: Some(get_service_names_and_types_trampoline::<R>),
        // phase-381 W3 — the last six. The trait defaults are `Unsupported`, so
        // a backend with no graph still answers "cannot tell you" rather than
        // an empty graph.
        get_publisher_names_and_types_by_node: Some(
            get_publisher_names_and_types_by_node_trampoline::<R>,
        ),
        get_subscriber_names_and_types_by_node: Some(
            get_subscriber_names_and_types_by_node_trampoline::<R>,
        ),
        get_service_names_and_types_by_node: Some(
            get_service_names_and_types_by_node_trampoline::<R>,
        ),
        get_client_names_and_types_by_node: Some(
            get_client_names_and_types_by_node_trampoline::<R>,
        ),
        get_publishers_info_by_topic: Some(get_publishers_info_by_topic_trampoline::<R>),
        get_subscriptions_info_by_topic: Some(get_subscriptions_info_by_topic_trampoline::<R>),
        // RFC-0088 D4 / phase-421 W2 — every Rust backend answers with its own
        // `Session::SERIALIZATION_FORMAT`, so a bridge image can ask each
        // session rather than trusting one image-wide constant. The trait
        // default is `"cdr"`; a backend that speaks something else overrides
        // it and this slot reports the override with no work here.
        get_serialization_format: Some(get_serialization_format_trampoline::<R>),
        ..EMPTY_VTABLE
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

unsafe extern "C" fn create_session_trampoline<R: RustBackend>(
    locator: *const core::ffi::c_char,
    mode: u8,
    domain_id: u32,
    node_name: *const core::ffi::c_char,
    options: *const NrosRmwSessionOptions,
    out: *mut NrosRmwSession,
) -> NrosRmwRet {
    if out.is_null() {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    // phase-206 W3 — marshal the caller's session properties into the slice
    // `RmwConfig` takes. This used to read `let _ = options;` with
    // `properties: &[]` hard-coded, which made every backend-specific
    // transport setting unreachable from C and C++: the Rust trait had carried
    // `properties` since it existed, and only a hosted Rust caller building an
    // `RmwConfig` by hand could fill it.
    //
    // Refused rather than truncated (each is INVALID_ARGUMENT):
    //   - `property_count > 0` with a NULL `properties` pointer,
    //   - more than `RMW_SESSION_MAX_PROPERTIES` entries,
    //   - a NULL, empty, or non-UTF-8 key or value.
    // Dropping any of these silently is the failure this work item exists to
    // remove; a caller that cannot see its configuration was ignored debugs
    // the transport instead of the typo.
    let mut prop_buf: [(&str, &str); MAX_SESSION_PROPERTIES] = [("", ""); MAX_SESSION_PROPERTIES];
    let prop_count = match unsafe { collect_session_properties(options, &mut prop_buf) } {
        Ok(n) => n,
        Err(ret) => return ret,
    };
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
        properties: &prop_buf[..prop_count],
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

unsafe extern "C" fn destroy_session_trampoline<R: RustBackend>(
    session: *mut NrosRmwSession,
) -> NrosRmwRet {
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
    node: *const NrosRmwNode,
    // phase-406 W1 — the identity arrives as ONE argument, in upstream's
    // position. Unpacked immediately below so the body is unchanged.
    type_support: *const crate::generated::rmw_message_type_support_t,
    topic_name: *const core::ffi::c_char,
    domain_id: u32,
    qos: *const NrosRmwQos,
    options: *const rmw_publisher_options_t,
    out: *mut NrosRmwPublisher,
) -> NrosRmwRet {
    // phase-406 W1 — unpack once, so the body below is untouched. A NULL
    // `type_support` is INVALID_ARGUMENT rather than a silent empty type: the
    // identity is what the backend keys its entity on, and an entity created
    // with no type is a subscription that matches nothing and reports nothing.
    let Some(ts) = (unsafe { type_support.as_ref() }) else {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    };
    let (type_name, type_hash) = (ts.type_name, ts.type_hash);
    if out.is_null() || qos.is_null() {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    // Phase 376 W5/B1 — the node carries both halves now: its own identity, and
    // the route to its session (upstream's `context`). Reading the name off the
    // session was the `entity_view` fabrication, and it is gone.
    if node.is_null() {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    let session = unsafe { (*node).session };
    let Some(s) = (unsafe { session_mut::<R::Session>(session) }) else {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    };
    let node_name = unsafe { node_name_of(node) };
    let namespace = unsafe { node_namespace_of(node) };
    let topic = TopicInfo {
        name: unsafe { cstr_to_str(topic_name) },
        type_name: unsafe { cstr_to_str(type_name) },
        type_hash: unsafe { cstr_to_str(type_hash) },
        domain_id,
        node_name,
        namespace,
        // Publisher side — the receive-buffer hint is subscription-only.
        rx_buffer_hint: 0,
        // phase-301 (issue 0240) — express hint from the NULLable options
        // struct (NULL = default, not express).
        tx_express: unsafe { options.as_ref() }.is_some_and(|o| o.tx_express != 0),
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
) -> NrosRmwRet {
    let _ = unsafe { take_box::<R::Publisher>(publisher_data_mut(publisher)) };

    NROS_RMW_RET_OK
}

unsafe extern "C" fn publish_trampoline<R: RustBackend>(
    publisher: *const NrosRmwPublisher,
    // phase-406 W2 — by VALUE; unpacked so the body is unchanged.
    payload: crate::generated::rmw_byte_span_t,
) -> NrosRmwRet {
    let (data, len) = (payload.data, payload.len);
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

unsafe extern "C" fn create_subscription_trampoline<R: RustBackend>(
    node: *const NrosRmwNode,
    // phase-406 W1 — the identity arrives as ONE argument, in upstream's
    // position. Unpacked immediately below so the body is unchanged.
    type_support: *const crate::generated::rmw_message_type_support_t,
    topic_name: *const core::ffi::c_char,
    domain_id: u32,
    qos: *const NrosRmwQos,
    options: *const rmw_subscription_options_t,
    out: *mut NrosRmwSubscription,
) -> NrosRmwRet {
    // phase-406 W1 — unpack once, so the body below is untouched. A NULL
    // `type_support` is INVALID_ARGUMENT rather than a silent empty type: the
    // identity is what the backend keys its entity on, and an entity created
    // with no type is a subscription that matches nothing and reports nothing.
    let Some(ts) = (unsafe { type_support.as_ref() }) else {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    };
    let (type_name, type_hash) = (ts.type_name, ts.type_hash);
    if out.is_null() || qos.is_null() {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    // Phase 376 W5/B1 — the node carries both halves now: its own identity, and
    // the route to its session (upstream's `context`). Reading the name off the
    // session was the `entity_view` fabrication, and it is gone.
    if node.is_null() {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    let session = unsafe { (*node).session };
    let Some(s) = (unsafe { session_mut::<R::Session>(session) }) else {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    };
    let node_name = unsafe { node_name_of(node) };
    let namespace = unsafe { node_namespace_of(node) };
    let topic = TopicInfo {
        name: unsafe { cstr_to_str(topic_name) },
        type_name: unsafe { cstr_to_str(type_name) },
        type_hash: unsafe { cstr_to_str(type_hash) },
        domain_id,
        node_name,
        namespace,
        // Phase 231 (RFC-0038) / phase-301 (issue 0240) — receive-buffer size
        // hint from the NULLable options struct (NULL = unset), so the backend
        // can size-class its receive storage.
        rx_buffer_hint: unsafe { options.as_ref() }.map_or(0, |o| o.rx_buffer_hint as usize),
        // Subscription side — express is a publisher-only hint.
        tx_express: false,
    };
    let qos_settings = qos_from_cffi(unsafe { &*qos });
    match Session::create_subscription(s, &topic, qos_settings) {
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

unsafe extern "C" fn destroy_subscription_trampoline<R: RustBackend>(
    subscriber: *mut NrosRmwSubscription,
) -> NrosRmwRet {
    let slot = unsafe { subscription_data_mut(subscriber) };
    #[cfg(all(target_os = "none", not(feature = "std")))]
    {
        if unsafe {
            static_subscriber_storage::take::<R::Subscription>(*slot as *mut R::Subscription)
        } {
            *slot = core::ptr::null_mut();
            return NROS_RMW_RET_OK;
        }
    }
    let _ = unsafe { take_box::<R::Subscription>(slot) };

    NROS_RMW_RET_OK
}

unsafe extern "C" fn take_trampoline<R: RustBackend>(
    subscriber: *const NrosRmwSubscription,
    // phase-406 W2 — by POINTER; `capacity` in, `len` out.
    out: *mut crate::generated::rmw_mut_byte_span_t,
    taken: *mut bool,
) -> NrosRmwRet {
    let Some(span) = (unsafe { out.as_mut() }) else {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    };
    let (buf, buf_len) = (span.data, span.capacity);
    let out_len: *mut usize = &mut span.len;
    // Phase 376 W3.b/W3.d step A — upstream's `rmw_take` shape: status in the
    // return, `taken` and the byte count in out-parameters. `Ok(None)` is no
    // longer the NO_DATA sentinel.
    if out_len.is_null() || taken.is_null() {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    let Some(s) = (unsafe { subscription_mut::<R::Subscription>(subscriber) }) else {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    };
    if buf.is_null() && buf_len != 0 {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    let slice = unsafe { core::slice::from_raw_parts_mut(buf, buf_len) };
    let key = unsafe { (*subscriber).backend_data as usize };

    // SAFETY (all four writes below): both pointers checked non-null above.
    #[cfg(feature = "safety-e2e")]
    if crate::take_cffi_integrity_request(key) {
        return match Subscription::take_validated(s, slice) {
            Ok(Some((n, status))) => {
                crate::store_cffi_integrity_status(key, status);
                unsafe {
                    *out_len = n;
                    *taken = true;
                }
                NROS_RMW_RET_OK
            }
            Ok(None) => {
                unsafe { *taken = false };
                NROS_RMW_RET_OK
            }
            Err(e) => ret_from_error(&e),
        };
    }

    match Subscription::take_serialized_with_info(s, slice) {
        Ok(Some((n, info))) => {
            crate::store_cffi_message_info(key, info);
            unsafe {
                *out_len = n;
                *taken = true;
            }
            NROS_RMW_RET_OK
        }
        Ok(None) => {
            unsafe { *taken = false };
            NROS_RMW_RET_OK
        }
        Err(e) => ret_from_error(&e),
    }
}

unsafe extern "C" fn has_data_trampoline<R: RustBackend>(
    subscription: *mut NrosRmwSubscription,
    out_has_data: *mut bool,
) -> NrosRmwRet {
    // Phase 376 W3.d step A — the flag moves to an out-parameter so the return
    // carries only a status. Written on OK only.
    if out_has_data.is_null() {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    // `subscription_ref`, not `_mut`: the probe is logically read-only and the
    // header says a backend must not mutate state here.
    let Some(s) = (unsafe { subscription_ref::<R::Subscription>(subscription) }) else {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    };
    // SAFETY: checked non-null above.
    unsafe { *out_has_data = Subscription::has_data(s) };
    NROS_RMW_RET_OK
}

// Phase 231 (RFC-0038) — in-place subscription take across the C ABI.

unsafe extern "C" fn subscription_supports_in_place_trampoline<R: RustBackend>(
    subscriber: *mut NrosRmwSubscription,
    out_supports: *mut bool,
) -> NrosRmwRet {
    // Phase 376 W3.d step A — capability out, status returned.
    if out_supports.is_null() {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    let Some(s) = (unsafe { subscription_ref::<R::Subscription>(subscriber) }) else {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    };
    // SAFETY: checked non-null above.
    unsafe { *out_supports = Subscription::supports_process_in_place(s) };
    NROS_RMW_RET_OK
}

unsafe extern "C" fn process_raw_in_place_trampoline<R: RustBackend>(
    subscriber: *mut NrosRmwSubscription,
    ctx: *mut core::ffi::c_void,
    cb: Option<
        unsafe extern "C" fn(
            ctx: *mut core::ffi::c_void,
            message: crate::generated::rmw_byte_span_t,
        ),
    >,
    out_processed: *mut bool,
) -> NrosRmwRet {
    // Phase 376 W3.d step A — "did it process one" is the out-parameter, and
    // `Ok(false)` is no longer reported as the NO_DATA sentinel.
    if out_processed.is_null() {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    let Some(s) = (unsafe { subscription_mut::<R::Subscription>(subscriber) }) else {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    };
    let Some(cb) = cb else {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    };
    match Subscription::process_raw_in_place(s, |raw| unsafe {
        cb(
            ctx,
            crate::generated::rmw_byte_span_t {
                data: raw.as_ptr(),
                len: raw.len(),
            },
        )
    }) {
        Ok(processed) => {
            // SAFETY: checked non-null above.
            unsafe { *out_processed = processed };
            NROS_RMW_RET_OK
        }
        Err(e) => ret_from_error(&e),
    }
}

// ============================================================================
// Trampolines — service server
// ============================================================================

unsafe extern "C" fn create_service_trampoline<R: RustBackend>(
    node: *const NrosRmwNode,
    // phase-406 W1 — the identity arrives as ONE argument, in upstream's
    // position. Unpacked immediately below so the body is unchanged.
    type_support: *const crate::generated::rmw_service_type_support_t,
    service_name: *const core::ffi::c_char,
    domain_id: u32,
    qos: *const NrosRmwQos,
    out: *mut NrosRmwService,
) -> NrosRmwRet {
    // phase-406 W1 — unpack once, so the body below is untouched. A NULL
    // `type_support` is INVALID_ARGUMENT rather than a silent empty type: the
    // identity is what the backend keys its entity on, and an entity created
    // with no type is a subscription that matches nothing and reports nothing.
    let Some(ts) = (unsafe { type_support.as_ref() }) else {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    };
    let (type_name, type_hash) = (ts.type_name, ts.type_hash);
    if out.is_null() || qos.is_null() {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    // Phase 376 W5/B1 — the node carries both halves now: its own identity, and
    // the route to its session (upstream's `context`). Reading the name off the
    // session was the `entity_view` fabrication, and it is gone.
    if node.is_null() {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    let session = unsafe { (*node).session };
    let Some(s) = (unsafe { session_mut::<R::Session>(session) }) else {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    };
    let node_name = unsafe { node_name_of(node) };
    let namespace = unsafe { node_namespace_of(node) };
    let info = nros_rmw::ServiceInfo {
        name: unsafe { cstr_to_str(service_name) },
        type_name: unsafe { cstr_to_str(type_name) },
        type_hash: unsafe { cstr_to_str(type_hash) },
        domain_id,
        node_name,
        namespace,
    };
    let qos_settings = qos_from_cffi(unsafe { &*qos });
    match Session::create_service(s, &info, qos_settings) {
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

unsafe extern "C" fn destroy_service_trampoline<R: RustBackend>(
    server: *mut NrosRmwService,
) -> NrosRmwRet {
    let _ = unsafe { take_box::<R::Service>(service_data_mut(server)) };

    NROS_RMW_RET_OK
}

unsafe extern "C" fn take_request_trampoline<R: RustBackend>(
    server: *const NrosRmwService,
    request: *mut crate::generated::rmw_mut_byte_span_t,
    seq_out: *mut i64,
    taken: *mut bool,
) -> NrosRmwRet {
    // Phase 376 W3.b/W3.d step A — upstream `rmw_take_request`'s shape;
    // phase-406 W2 — the byte range is one span, and `written` lands in it.
    if request.is_null() || taken.is_null() {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    let Some(s) = (unsafe { service_mut::<R::Service>(server) }) else {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    };
    let (buf, buf_len) = unsafe { ((*request).data, (*request).capacity) };
    let out_len = unsafe { &raw mut (*request).len };
    if (buf.is_null() && buf_len != 0) || seq_out.is_null() {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    let slice = unsafe { core::slice::from_raw_parts_mut(buf, buf_len) };
    let buf_start = slice.as_ptr() as usize;
    match ServiceTrait::take_request(s, slice) {
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
            // SAFETY: both checked non-null above.
            unsafe {
                *out_len = len;
                *taken = true;
            }
            NROS_RMW_RET_OK
        }
        Ok(None) => {
            // SAFETY: checked non-null above.
            unsafe { *taken = false };
            NROS_RMW_RET_OK
        }
        Err(e) => ret_from_error(&e),
    }
}

unsafe extern "C" fn has_request_trampoline<R: RustBackend>(
    server: *mut NrosRmwService,
    out_has_request: *mut bool,
) -> NrosRmwRet {
    // Phase 376 W3.d step A — see `has_data_trampoline`.
    if out_has_request.is_null() {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    let Some(s) = (unsafe { service_ref::<R::Service>(server) }) else {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    };
    // SAFETY: checked non-null above.
    unsafe { *out_has_request = ServiceTrait::has_request(s) };
    NROS_RMW_RET_OK
}

unsafe extern "C" fn send_response_trampoline<R: RustBackend>(
    server: *const NrosRmwService,
    seq: i64,
    response: crate::generated::rmw_byte_span_t,
) -> NrosRmwRet {
    let Some(s) = (unsafe { service_mut::<R::Service>(server) }) else {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    };
    let (data, len) = (response.data, response.len);
    if data.is_null() && len != 0 {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    let slice = unsafe { core::slice::from_raw_parts(data, len) };
    match ServiceTrait::send_response(s, seq, slice) {
        Ok(()) => NROS_RMW_RET_OK,
        Err(e) => ret_from_error(&e),
    }
}

// ============================================================================
// Trampolines — service client
// ============================================================================

unsafe extern "C" fn create_client_trampoline<R: RustBackend>(
    node: *const NrosRmwNode,
    // phase-406 W1 — the identity arrives as ONE argument, in upstream's
    // position. Unpacked immediately below so the body is unchanged.
    type_support: *const crate::generated::rmw_service_type_support_t,
    service_name: *const core::ffi::c_char,
    domain_id: u32,
    qos: *const NrosRmwQos,
    out: *mut NrosRmwClient,
) -> NrosRmwRet {
    // phase-406 W1 — unpack once, so the body below is untouched. A NULL
    // `type_support` is INVALID_ARGUMENT rather than a silent empty type: the
    // identity is what the backend keys its entity on, and an entity created
    // with no type is a subscription that matches nothing and reports nothing.
    let Some(ts) = (unsafe { type_support.as_ref() }) else {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    };
    let (type_name, type_hash) = (ts.type_name, ts.type_hash);
    if out.is_null() || qos.is_null() {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    // Phase 376 W5/B1 — the node carries both halves now: its own identity, and
    // the route to its session (upstream's `context`). Reading the name off the
    // session was the `entity_view` fabrication, and it is gone.
    if node.is_null() {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    let session = unsafe { (*node).session };
    let Some(s) = (unsafe { session_mut::<R::Session>(session) }) else {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    };
    let node_name = unsafe { node_name_of(node) };
    let namespace = unsafe { node_namespace_of(node) };
    let info = nros_rmw::ServiceInfo {
        name: unsafe { cstr_to_str(service_name) },
        type_name: unsafe { cstr_to_str(type_name) },
        type_hash: unsafe { cstr_to_str(type_hash) },
        domain_id,
        node_name,
        namespace,
    };
    let qos_settings = qos_from_cffi(unsafe { &*qos });
    match Session::create_client(s, &info, qos_settings) {
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

unsafe extern "C" fn destroy_client_trampoline<R: RustBackend>(
    client: *mut NrosRmwClient,
) -> NrosRmwRet {
    let _ = unsafe { take_box::<R::Client>(client_data_mut(client)) };

    NROS_RMW_RET_OK
}

// Non-blocking send/recv trampolines — the one request/reply path
// (phase-301: the blocking `call_raw` slot is deleted).
unsafe extern "C" fn send_request_trampoline<R: RustBackend>(
    client: *const NrosRmwClient,
    request: crate::generated::rmw_byte_span_t,
    sequence_id: *mut i64,
) -> NrosRmwRet {
    let Some(c) = (unsafe { client_mut::<R::Client>(client) }) else {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    };
    let (request, req_len) = (request.data, request.len);
    if request.is_null() && req_len != 0 {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    let req = unsafe { core::slice::from_raw_parts(request, req_len) };
    match ClientTrait::send_request_raw(c, req) {
        Ok(seq) => {
            // Issue 0778 — hand the id back. NULL is tolerated for a caller
            // that genuinely has one call in flight and does not want it.
            if !sequence_id.is_null() {
                // SAFETY: checked non-null.
                unsafe { *sequence_id = seq };
            }
            NROS_RMW_RET_OK
        }
        Err(e) => ret_from_error(&e),
    }
}

unsafe extern "C" fn take_response_trampoline<R: RustBackend>(
    client: *const NrosRmwClient,
    reply: *mut crate::generated::rmw_mut_byte_span_t,
    seq_out: *mut i64,
    taken: *mut bool,
) -> NrosRmwRet {
    // Phase 376 W3.b/W3.d step A — upstream `rmw_take_response`'s shape;
    // phase-406 W2 — one span in place of the buf/len/out_len triple.
    if reply.is_null() || taken.is_null() {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    let Some(c) = (unsafe { client_mut::<R::Client>(client) }) else {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    };
    let (reply_buf, reply_buf_len) = unsafe { ((*reply).data, (*reply).capacity) };
    let out_len = unsafe { &raw mut (*reply).len };
    if reply_buf.is_null() && reply_buf_len != 0 {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    let reply = unsafe { core::slice::from_raw_parts_mut(reply_buf, reply_buf_len) };
    match ClientTrait::take_response_raw(c, reply) {
        Ok(Some((n, seq))) => {
            // SAFETY: both checked non-null above.
            unsafe {
                *out_len = n;
                *taken = true;
                if !seq_out.is_null() {
                    *seq_out = seq;
                }
            }
            NROS_RMW_RET_OK
        }
        Ok(None) => {
            // SAFETY: checked non-null above.
            unsafe { *taken = false };
            NROS_RMW_RET_OK
        }
        Err(e) => ret_from_error(&e),
    }
}

// ============================================================================
// Event-callback bridge (Phase 115.L.0.events)
// ============================================================================
//
// Events / liveliness / deadline are wired through the vtable (see the
// `register_subscription_event` / `register_publisher_event` /
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
    // EventKind tags must round-trip 0..=4 between the generated C
    // discriminants and the `#[repr(u8)]` trait enum.
    use crate::rmw_event_type_t as ck;
    assert!(
        ck::NROS_RMW_EVENT_LIVELINESS_CHANGED == nros_rmw::EventKind::LivelinessChanged as ck::Type
    );
    assert!(
        ck::NROS_RMW_EVENT_REQUESTED_DEADLINE_MISSED
            == nros_rmw::EventKind::RequestedDeadlineMissed as ck::Type
    );
    assert!(ck::NROS_RMW_EVENT_MESSAGE_LOST == nros_rmw::EventKind::MessageLost as ck::Type);
    assert!(ck::NROS_RMW_EVENT_LIVELINESS_LOST == nros_rmw::EventKind::LivelinessLost as ck::Type);
    assert!(
        ck::NROS_RMW_EVENT_OFFERED_DEADLINE_MISSED
            == nros_rmw::EventKind::OfferedDeadlineMissed as ck::Type
    );
};

unsafe extern "C" fn subscription_event_init_trampoline<R: RustBackend>(
    subscriber: *const NrosRmwSubscription,
    kind: NrosRmwEventKind,
    deadline_ms: u32,
    cb: NrosRmwEventCallback,
    user_context: *mut c_void,
) -> NrosRmwRet {
    let Some(s) = (unsafe { subscription_mut::<R::Subscription>(subscriber) }) else {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    };
    // Explicit conversion, not a transmute: `NrosRmwEventKind` is
    // C-uint-sized (#238) while trait `EventKind` is `#[repr(u8)]` —
    // reinterpreting bytes would be UB.
    let trait_kind: nros_rmw::EventKind = event_kind_from_c(kind);
    // The generated callback type is nullable; a NULL callback can't be
    // forwarded into the trait's non-null callback surface.
    let Some(cb) = cb else {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    };
    // SAFETY: see module-level note. `cb`'s parameter types
    // `(NrosRmwEventKind, *const NrosRmwEventPayload, *mut c_void)` are
    // ABI-compatible with the trait's `(EventKind, *const c_void, *mut c_void)`.
    let trait_cb: nros_rmw::EventCallback = unsafe { core::mem::transmute(cb) };
    let res = unsafe {
        Subscription::register_event_callback(s, trait_kind, deadline_ms, trait_cb, user_context)
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

unsafe extern "C" fn publisher_event_init_trampoline<R: RustBackend>(
    publisher: *const NrosRmwPublisher,
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
    // Explicit conversion, not a transmute (see subscriber trampoline / #238).
    let trait_kind: nros_rmw::EventKind = event_kind_from_c(kind);
    let Some(cb) = cb else {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    };
    let trait_cb: nros_rmw::EventCallback = unsafe { core::mem::transmute(cb) };
    let res = unsafe {
        Publisher::register_event_callback(p, trait_kind, deadline_ms, trait_cb, user_context)
    };
    match res {
        Ok(()) => NROS_RMW_RET_OK,
        Err(_) => NROS_RMW_RET_UNSUPPORTED,
    }
}

unsafe extern "C" fn publisher_assert_liveliness_trampoline<R: RustBackend>(
    publisher: *const NrosRmwPublisher,
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
    out_ms: *mut u32,
    has_deadline: *mut bool,
) -> NrosRmwRet {
    // Phase 376 W3.d step A — value and presence out, status returned.
    if out_ms.is_null() || has_deadline.is_null() {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    // A NULL session was previously reported as `-1`, indistinguishable from a
    // link with nothing scheduled. It is an invalid argument.
    let Some(s) = (unsafe { session_ref::<R::Session>(session) }) else {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    };
    match Session::next_deadline_ms(s) {
        Some(ms) => {
            // SAFETY: both checked non-null above.
            unsafe {
                *out_ms = ms;
                *has_deadline = true;
            }
        }
        None => {
            // SAFETY: checked non-null above.
            unsafe { *has_deadline = false };
        }
    }
    NROS_RMW_RET_OK
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

unsafe extern "C" fn service_server_is_available_trampoline<R: RustBackend>(
    client: *const NrosRmwClient,
    out_available: *mut bool,
) -> NrosRmwRet {
    // Phase 124.C.1 — delegate to the Rust backend's
    // `ClientTrait::server_available` impl. Default trait
    // body returns `Err(TransportError::Unsupported)`; concrete
    // backends opt in by overriding.
    //
    // Phase 376 W3.d step A — status in the return, answer in the
    // out-parameter. `*out_available` is written ONLY on OK, so an
    // error leaves whatever the caller initialised it to; that is
    // upstream's contract and it is also what keeps a caller who
    // ignores the status from reading a stale `true` as fresh.
    if out_available.is_null() {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    let Some(c) = (unsafe { client_mut::<R::Client>(client) }) else {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    };
    match ClientTrait::service_is_ready(c) {
        Ok(available) => {
            // SAFETY: checked non-null above; the caller owns a `bool`.
            unsafe { *out_available = available };
            NROS_RMW_RET_OK
        }
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

/// phase-381 W3 — bridge the C visitor to the Rust `Session::get_node_names`.
///
/// The C side hands a `rmw_node_visit_fn` plus an opaque `ctx`; the Rust trait
/// takes a closure. This wraps the former in the latter, and the strings it
/// passes are NUL-terminated on the stack because the C contract borrows them
/// for the call only — no allocation on either side of the seam.
///
/// A name that does not fit the stack buffer is SKIPPED rather than truncated:
/// a truncated node name is a different, plausible node.
unsafe extern "C" fn get_node_names_trampoline<R: RustBackend>(
    session: *const NrosRmwSession,
    // phase-406 W2 — the pair as one argument. The NODE visitor: its callback
    // takes an enclave, which the names-and-types one does not.
    visitor: crate::generated::rmw_node_visitor_t,
) -> NrosRmwRet {
    let (visit, ctx) = (visitor.visit, visitor.ctx);
    let Some(visit) = visit else {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    };
    let Some(s) = (unsafe { session_mut::<R::Session>(session.cast_mut()) }) else {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    };

    /// Longest node name / namespace this seam will pass through.
    const NAME_MAX: usize = 256;

    let mut cb = |name: &str, namespace: &str, enclave: Option<&str>| -> bool {
        let mut name_buf = [0u8; NAME_MAX];
        let mut ns_buf = [0u8; NAME_MAX];
        let mut enc_buf = [0u8; NAME_MAX];
        // `< NAME_MAX`, not `<= `: the buffer must hold the bytes AND the NUL
        // the C side reads to. Spelled as the strict compare clippy wants
        // (`int_plus_one`) rather than the `len + 1 <= cap` that says it.
        if name.len() >= NAME_MAX || namespace.len() >= NAME_MAX {
            return true; // skip this one, keep enumerating
        }
        name_buf[..name.len()].copy_from_slice(name.as_bytes());
        ns_buf[..namespace.len()].copy_from_slice(namespace.as_bytes());
        let enc_ptr = match enclave {
            Some(e) if e.len() < NAME_MAX => {
                enc_buf[..e.len()].copy_from_slice(e.as_bytes());
                enc_buf.as_ptr() as *const c_char
            }
            // NULL is the contract's "this backend does not track one", which
            // is what lets one slot answer both `rmw_get_node_names` and
            // `rmw_get_node_names_with_enclaves`.
            _ => core::ptr::null(),
        };
        unsafe {
            visit(
                ctx,
                name_buf.as_ptr() as *const c_char,
                ns_buf.as_ptr() as *const c_char,
                enc_ptr,
            )
        }
    };

    match Session::get_node_names(s, &mut cb) {
        Ok(()) => NROS_RMW_RET_OK,
        Err(e) => ret_from_error(&e),
    }
}

/// phase-381 W3 — `count_publishers` / `count_subscribers` share one body.
macro_rules! count_trampoline {
    ($name:ident, $method:ident) => {
        unsafe extern "C" fn $name<R: RustBackend>(
            session: *const NrosRmwSession,
            topic_name: *const c_char,
            count: *mut usize,
        ) -> NrosRmwRet {
            if topic_name.is_null() || count.is_null() {
                return NROS_RMW_RET_INVALID_ARGUMENT;
            }
            let Some(s) = (unsafe { session_mut::<R::Session>(session.cast_mut()) }) else {
                return NROS_RMW_RET_INVALID_ARGUMENT;
            };
            let Ok(topic) = (unsafe { core::ffi::CStr::from_ptr(topic_name) }).to_str() else {
                return NROS_RMW_RET_INVALID_ARGUMENT;
            };
            match Session::$method(s, topic) {
                Ok(n) => {
                    unsafe { *count = n };
                    NROS_RMW_RET_OK
                }
                Err(e) => ret_from_error(&e),
            }
        }
    };
}

count_trampoline!(count_publishers_trampoline, count_publishers);
count_trampoline!(count_subscribers_trampoline, count_subscribers);

/// phase-381 W3 — `get_topic_names_and_types` / `get_service_names_and_types`.
///
/// The C visitor takes `(name, types[], types_count)`. Rust hands over
/// `&[&str]`, so each call builds a bounded array of NUL-terminated pointers on
/// the stack — the strings are borrowed for the call only, which is the
/// contract on both sides.
///
/// A name or type that does not fit is SKIPPED, never truncated: a truncated
/// type name is a different, plausible type.
macro_rules! names_and_types_trampoline {
    // The topic form carries `no_demangle`; the service form does not — that
    // asymmetry is upstream's, mirrored rather than smoothed over.
    ($name:ident, $method:ident, demangle_flag) => {
        unsafe extern "C" fn $name<R: RustBackend>(
            session: *const NrosRmwSession,
            no_demangle: bool,
            // phase-406 W2 — the pair as one argument.
            visitor: crate::generated::rmw_names_and_types_visitor_t,
        ) -> NrosRmwRet {
            // `no_demangle` asks for the WIRE spelling. This backend reports ROS
            // names, so honouring it would mean re-mangling what was just
            // demangled; refused rather than silently ignored, because ignoring
            // it answers a different question than the caller asked.
            if no_demangle {
                return NROS_RMW_RET_UNSUPPORTED;
            }
            $crate::names_and_types_body!(R, $method, session, visitor)
        }
    };
    ($name:ident, $method:ident) => {
        unsafe extern "C" fn $name<R: RustBackend>(
            session: *const NrosRmwSession,
            // phase-406 W2 — the pair as one argument.
            visitor: crate::generated::rmw_names_and_types_visitor_t,
        ) -> NrosRmwRet {
            $crate::names_and_types_body!(R, $method, session, visitor)
        }
    };
}

/// The shared body, so the two arms above differ only by the parameter list.
#[macro_export]
#[doc(hidden)]
macro_rules! names_and_types_body {
    ($R:ident, $method:ident, $session:ident, $visitor:ident) => {{
        // phase-406 W2 — unpack HERE: a `let` in the caller's expansion is not
        // visible in this one (macro hygiene), so the struct crosses instead.
        let (visit_fn, ctx) = ($visitor.visit, $visitor.ctx);
        let Some(visit) = visit_fn else {
            return NROS_RMW_RET_INVALID_ARGUMENT;
        };
        let Some(s) = (unsafe { session_mut::<$R::Session>($session.cast_mut()) }) else {
            return NROS_RMW_RET_INVALID_ARGUMENT;
        };

        const NAME_MAX: usize = 256;
        const TYPES_MAX: usize = 8;

        let mut cb = |name: &str, types: &[&str]| -> bool {
            let mut name_buf = [0u8; NAME_MAX];
            if name.len() >= NAME_MAX {
                return true; // skip, keep enumerating
            }
            name_buf[..name.len()].copy_from_slice(name.as_bytes());

            let mut type_bufs = [[0u8; NAME_MAX]; TYPES_MAX];
            let mut ptrs: [*const c_char; TYPES_MAX] = [core::ptr::null(); TYPES_MAX];
            let mut n = 0usize;
            for t in types.iter().take(TYPES_MAX) {
                if t.len() >= NAME_MAX {
                    continue;
                }
                type_bufs[n][..t.len()].copy_from_slice(t.as_bytes());
                ptrs[n] = type_bufs[n].as_ptr() as *const c_char;
                n += 1;
            }
            unsafe { visit(ctx, name_buf.as_ptr() as *const c_char, ptrs.as_ptr(), n) }
        };

        match Session::$method(s, &mut cb) {
            Ok(()) => NROS_RMW_RET_OK,
            Err(e) => ret_from_error(&e),
        }
    }};
}

names_and_types_trampoline!(
    get_topic_names_and_types_trampoline,
    get_topic_names_and_types,
    demangle_flag
);
names_and_types_trampoline!(
    get_service_names_and_types_trampoline,
    get_service_names_and_types
);

/// phase-381 W3 — the four `*_by_node` slots, one body.
///
/// Longhand callers, shared implementation: they differ only by which
/// `GraphEntityKind` they pass, and four copies of this marshalling is four
/// chances for one of them to drift.
///
/// # Safety
/// Same contract as the slots that call it.
unsafe fn by_node_impl<R: RustBackend>(
    session: *const NrosRmwSession,
    node_name: *const c_char,
    node_namespace: *const c_char,
    no_demangle: bool,
    // phase-406 W2 — the pair as one argument.
    visitor: crate::generated::rmw_names_and_types_visitor_t,
    kind: nros_rmw::GraphEntityKind,
) -> NrosRmwRet {
    let (visit, ctx) = (visitor.visit, visitor.ctx);
    // `no_demangle` asks for the WIRE spelling; we report ROS names, so
    // honouring it would mean re-mangling what was just demangled. Refused
    // rather than ignored — ignoring answers a different question than asked.
    if no_demangle {
        return NROS_RMW_RET_UNSUPPORTED;
    }
    if node_name.is_null() || node_namespace.is_null() {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    let Some(visit) = visit else {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    };
    let Some(s) = (unsafe { session_mut::<R::Session>(session.cast_mut()) }) else {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    };
    let (Ok(name), Ok(ns)) = (
        unsafe { core::ffi::CStr::from_ptr(node_name) }.to_str(),
        unsafe { core::ffi::CStr::from_ptr(node_namespace) }.to_str(),
    ) else {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    };

    const NAME_MAX: usize = 256;
    const TYPES_MAX: usize = 8;
    let mut cb = |n: &str, types: &[&str]| -> bool {
        let mut name_buf = [0u8; NAME_MAX];
        if n.len() >= NAME_MAX {
            return true;
        }
        name_buf[..n.len()].copy_from_slice(n.as_bytes());
        let mut type_bufs = [[0u8; NAME_MAX]; TYPES_MAX];
        let mut ptrs: [*const c_char; TYPES_MAX] = [core::ptr::null(); TYPES_MAX];
        let mut count = 0usize;
        for t in types.iter().take(TYPES_MAX) {
            if t.len() >= NAME_MAX {
                continue;
            }
            type_bufs[count][..t.len()].copy_from_slice(t.as_bytes());
            ptrs[count] = type_bufs[count].as_ptr() as *const c_char;
            count += 1;
        }
        unsafe {
            visit(
                ctx,
                name_buf.as_ptr() as *const c_char,
                ptrs.as_ptr(),
                count,
            )
        }
    };
    match Session::get_names_and_types_by_node(s, kind, name, ns, &mut cb) {
        Ok(()) => NROS_RMW_RET_OK,
        Err(e) => ret_from_error(&e),
    }
}

macro_rules! by_node_trampoline {
    // Upstream gives `no_demangle` to the publisher and subscriber forms and
    // NOT to the service and client ones. Mirrored rather than smoothed over —
    // RFC-0054 makes these headers the ABI SSoT, so an asymmetry upstream has
    // is one we have.
    ($name:ident, $kind:ident, no_demangle) => {
        unsafe extern "C" fn $name<R: RustBackend>(
            session: *const NrosRmwSession,
            node_name: *const c_char,
            node_namespace: *const c_char,
            no_demangle: bool,
            // phase-406 W2 — the pair as one argument.
            visitor: crate::generated::rmw_names_and_types_visitor_t,
        ) -> NrosRmwRet {
            unsafe {
                by_node_impl::<R>(
                    session,
                    node_name,
                    node_namespace,
                    no_demangle,
                    visitor,
                    nros_rmw::GraphEntityKind::$kind,
                )
            }
        }
    };
    // The service and client forms, which upstream gives no `no_demangle`.
    ($name:ident, $kind:ident) => {
        unsafe extern "C" fn $name<R: RustBackend>(
            session: *const NrosRmwSession,
            node_name: *const c_char,
            node_namespace: *const c_char,
            // phase-406 W2 — the pair as one argument.
            visitor: crate::generated::rmw_names_and_types_visitor_t,
        ) -> NrosRmwRet {
            unsafe {
                by_node_impl::<R>(
                    session,
                    node_name,
                    node_namespace,
                    false,
                    visitor,
                    nros_rmw::GraphEntityKind::$kind,
                )
            }
        }
    };
}

by_node_trampoline!(
    get_publisher_names_and_types_by_node_trampoline,
    Publisher,
    no_demangle
);
by_node_trampoline!(
    get_subscriber_names_and_types_by_node_trampoline,
    Subscriber,
    no_demangle
);
by_node_trampoline!(get_service_names_and_types_by_node_trampoline, Service);
by_node_trampoline!(get_client_names_and_types_by_node_trampoline, Client);

/// phase-381 W3 — the two `*_info_by_topic` slots, one body.
///
/// # Safety
/// Same contract as the slots that call it.
unsafe fn endpoint_info_impl<R: RustBackend>(
    session: *const NrosRmwSession,
    topic_name: *const c_char,
    no_mangle: bool,
    // phase-406 W2 — the pair as one argument.
    visitor: crate::generated::rmw_topic_endpoint_info_visitor_t,
    publishers: bool,
) -> NrosRmwRet {
    let (visit, ctx) = (visitor.visit, visitor.ctx);
    if no_mangle {
        return NROS_RMW_RET_UNSUPPORTED;
    }
    if topic_name.is_null() {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    let Some(visit) = visit else {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    };
    let Some(s) = (unsafe { session_mut::<R::Session>(session.cast_mut()) }) else {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    };
    let Ok(topic) = (unsafe { core::ffi::CStr::from_ptr(topic_name) }).to_str() else {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    };

    const NAME_MAX: usize = 256;
    let mut cb = |info: &nros_rmw::GraphEndpointInfo<'_>| -> bool {
        let mut name_buf = [0u8; NAME_MAX];
        let mut ns_buf = [0u8; NAME_MAX];
        let mut ty_buf = [0u8; NAME_MAX];
        if info.node_name.len() >= NAME_MAX
            || info.node_namespace.len() >= NAME_MAX
            || info.topic_type.len() >= NAME_MAX
        {
            return true; // skip, never truncate
        }
        name_buf[..info.node_name.len()].copy_from_slice(info.node_name.as_bytes());
        ns_buf[..info.node_namespace.len()].copy_from_slice(info.node_namespace.as_bytes());
        ty_buf[..info.topic_type.len()].copy_from_slice(info.topic_type.as_bytes());
        // The struct has no "qos known" flag, by design: the ABI expresses an
        // unreadable policy with the `*_UNKNOWN` sentinels and the contract on
        // `publisher_get_actual_qos` spells it out — write UNKNOWN for what you
        // cannot determine and return OK; `UNSUPPORTED` means no read-back AT
        // ALL, not "I know some of it".
        //
        // zenoh's liveliness token DOES carry a QoS chunk, but decoding it is
        // deferred rather than guessed: it is the DECLARING side's own profile
        // and this seam promises a GRANTED one, so shipping it now would be the
        // plausible wrong answer this slot exists to avoid. `GraphEndpointInfo::qos`
        // is `None` from every backend today, so this is always the UNKNOWN arm.
        let mut qos: rmw_qos_profile_t = unsafe { core::mem::zeroed() };
        qos.reliability = NROS_RMW_RELIABILITY_UNKNOWN as u8;
        qos.durability = NROS_RMW_DURABILITY_UNKNOWN as u8;
        qos.history = NROS_RMW_HISTORY_UNKNOWN as u8;
        qos.liveliness_kind = rmw_liveliness_kind_t::NROS_RMW_LIVELINESS_UNKNOWN as u8;
        let c_info = rmw_topic_endpoint_info_t {
            node_name: name_buf.as_ptr() as *const c_char,
            node_namespace: ns_buf.as_ptr() as *const c_char,
            topic_type: ty_buf.as_ptr() as *const c_char,
            endpoint_type: if info.is_publisher {
                rmw_endpoint_type_t::RMW_ENDPOINT_PUBLISHER
            } else {
                rmw_endpoint_type_t::RMW_ENDPOINT_SUBSCRIPTION
            },
            endpoint_gid: rmw_gid_t {
                implementation_identifier: core::ptr::null(),
                data: info.endpoint_gid,
            },
            qos_profile: qos,
        };
        unsafe { visit(ctx, &c_info as *const rmw_topic_endpoint_info_t) }
    };
    match Session::get_endpoint_info_by_topic(s, publishers, topic, &mut cb) {
        Ok(()) => NROS_RMW_RET_OK,
        Err(e) => ret_from_error(&e),
    }
}

macro_rules! endpoint_info_trampoline {
    ($name:ident, $publishers:expr) => {
        unsafe extern "C" fn $name<R: RustBackend>(
            session: *const NrosRmwSession,
            topic_name: *const c_char,
            no_mangle: bool,
            // phase-406 W2 — the pair as one argument.
            visitor: crate::generated::rmw_topic_endpoint_info_visitor_t,
        ) -> NrosRmwRet {
            unsafe { endpoint_info_impl::<R>(session, topic_name, no_mangle, visitor, $publishers) }
        }
    };
}

endpoint_info_trampoline!(get_publishers_info_by_topic_trampoline, true);
endpoint_info_trampoline!(get_subscriptions_info_by_topic_trampoline, false);

unsafe extern "C" fn publish_streamed_trampoline<R: RustBackend>(
    publisher: *mut NrosRmwPublisher,
    size_cb: Option<unsafe extern "C" fn(out_total_len: *mut usize, user_ctx: *mut c_void)>,
    chunk_cb: Option<
        unsafe extern "C" fn(
            out_buf: *mut u8,
            cap: usize,
            out_written: *mut usize,
            user_ctx: *mut c_void,
        ),
    >,
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
    // Generated ABI marks the callbacks nullable; the trait surface is not.
    let (Some(size_cb), Some(chunk_cb)) = (size_cb, chunk_cb) else {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    };
    // SAFETY: this trampoline is entered from the C ABI with the same
    // callback/user_ctx lifetime contract required by `Publisher::publish_streamed`.
    match unsafe { Publisher::publish_streamed(p, size_cb, chunk_cb, user_ctx) } {
        Ok(()) => NROS_RMW_RET_OK,
        Err(e) => ret_from_error(&e),
    }
}

unsafe extern "C" fn take_sequence_trampoline<R: RustBackend>(
    subscriber: *const NrosRmwSubscription,
    buf: *mut u8,
    per_msg_cap: usize,
    max_msgs: usize,
    out_lens: *mut usize,
    taken: *mut usize,
) -> NrosRmwRet {
    // Phase 376 W3.b/W3.d step A — the COUNT is an out-parameter, matching
    // upstream's `size_t *taken`.
    if taken.is_null() {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    // Phase 124.D.1 — delegate to the Rust backend's
    // `Subscription::take_sequence` impl. Default trait body
    // loop-drives `take_serialized`; concrete backends opt in by
    // overriding for a native batch take.
    let Some(s) = (unsafe { subscription_mut::<R::Subscription>(subscriber) }) else {
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
    match Subscription::take_sequence(s, buf_slice, per_msg_cap, max_msgs, lens_slice) {
        Ok(count) => {
            // SAFETY: checked non-null above.
            unsafe { *taken = count };
            NROS_RMW_RET_OK
        }
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
unsafe fn subscription_data_mut(subscriber: *mut NrosRmwSubscription) -> &'static mut *mut c_void {
    unsafe { &mut (*subscriber).backend_data }
}

#[inline]
unsafe fn service_data_mut(server: *mut NrosRmwService) -> &'static mut *mut c_void {
    unsafe { &mut (*server).backend_data }
}

#[inline]
unsafe fn client_data_mut(client: *mut NrosRmwClient) -> &'static mut *mut c_void {
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
unsafe fn publisher_ref<'a, T>(publisher: *const NrosRmwPublisher) -> Option<&'a T> {
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
unsafe fn subscription_mut<'a, T>(subscriber: *const NrosRmwSubscription) -> Option<&'a mut T> {
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
unsafe fn subscription_ref<'a, T>(subscriber: *const NrosRmwSubscription) -> Option<&'a T> {
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
unsafe fn service_mut<'a, T>(server: *const NrosRmwService) -> Option<&'a mut T> {
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
unsafe fn service_ref<'a, T>(server: *const NrosRmwService) -> Option<&'a T> {
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
unsafe fn client_mut<'a, T>(client: *const NrosRmwClient) -> Option<&'a mut T> {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// issue 0829 — the inbound edge RAISES a 0 policy back to the sentinel, so
    /// a C caller's "unstated" survives to the Rust backend that has to resolve
    /// it. Before this, `qos_from_cffi` folded 0 to RELIABLE / VOLATILE /
    /// KEEP_LAST here at the edge, which made every Rust backend answer the
    /// sentinel identically — the thing upstream deliberately does not do.
    ///
    /// This is the round trip: profile -> C struct -> profile.
    #[test]
    fn a_zero_filled_c_profile_reads_back_as_the_sentinel() {
        let raised = qos_from_cffi(&crate::NROS_RMW_QOS_PROFILE_SYSTEM_DEFAULT);
        assert_eq!(raised, nros_rmw::QoSProfile::QOS_PROFILE_SYSTEM_DEFAULT);
        assert!(raised.has_unresolved_system_default());
    }

    /// UNKNOWN is NOT the sentinel and must keep its old fold: it is a
    /// read-back artefact ("the backend could not determine this"), not a
    /// request, so there is no absence to carry.
    #[test]
    fn unknown_is_not_raised_to_the_sentinel() {
        let mut q = crate::NROS_RMW_QOS_PROFILE_SYSTEM_DEFAULT;
        q.reliability = crate::generated::NROS_RMW_RELIABILITY_UNKNOWN as u8;
        let raised = qos_from_cffi(&q);
        assert_eq!(
            raised.reliability,
            nros_rmw::QoSReliabilityPolicy::Reliable,
            "UNKNOWN must keep folding to the ROS default, not become a sentinel"
        );
    }
}
