//! C function table adapter for nros RMW backends.
//!
//! This crate provides a vtable-based bridge so that backends written in C,
//! C++, Zig, Ada, or any language with a C-compatible ABI can implement the
//! nros `Session` / `Publisher` / `Subscriber` / service traits without
//! writing Rust code.
//!
//! # Usage (C backend implementor)
//!
//! 1. Include `<nros/rmw_vtable.h>`
//! 2. Implement all function pointers in `nros_rmw_vtable_t`
//! 3. Call `nros_rmw_cffi_register(&my_vtable)` before creating sessions
//!
//! # Usage (Rust consumer)
//!
//! Enable the `rmw-cffi` feature on `nros` and use `Executor<CffiSession>`.

#![no_std]

use core::ffi::c_void;
use core::sync::atomic::Ordering;

use portable_atomic::AtomicPtr;

use nros_rmw::{
    Publisher, QosDurabilityPolicy, QosHistoryPolicy, QosReliabilityPolicy, QosSettings,
    ServiceClientTrait, ServiceInfo, ServiceRequest, ServiceServerTrait, Session, TopicInfo,
    TransportError,
};

// ============================================================================
// Phase 102.1 — `nros_rmw_ret_t` named return codes
// ============================================================================
//
// Mirrors the macro constants in `<nros/rmw_ret.h>`. The C side uses
// `#define` so future additions don't widen the type; the Rust side
// uses `pub const` so the same names are usable by Rust code that
// crosses the C-vtable boundary.

/// Signed 32-bit status code mirroring the C `nros_rmw_ret_t` typedef.
/// Zero on success; negative on error.
pub type NrosRmwRet = i32;

/// Operation completed successfully.
pub const NROS_RMW_RET_OK: NrosRmwRet = 0;
/// Generic failure not covered by a more specific code.
pub const NROS_RMW_RET_ERROR: NrosRmwRet = -1;
/// Operation deadline elapsed before completion.
pub const NROS_RMW_RET_TIMEOUT: NrosRmwRet = -2;
/// Memory allocation failed.
pub const NROS_RMW_RET_BAD_ALLOC: NrosRmwRet = -3;
/// Caller supplied a NULL pointer or an out-of-range value.
pub const NROS_RMW_RET_INVALID_ARGUMENT: NrosRmwRet = -4;
/// The backend does not implement this operation.
pub const NROS_RMW_RET_UNSUPPORTED: NrosRmwRet = -5;
/// QoS profiles incompatible in a way the backend cannot reconcile.
pub const NROS_RMW_RET_INCOMPATIBLE_QOS: NrosRmwRet = -6;
/// Topic, service, or action name failed validation.
pub const NROS_RMW_RET_TOPIC_NAME_INVALID: NrosRmwRet = -7;
/// Request referenced a node that does not exist in this session.
pub const NROS_RMW_RET_NODE_NAME_NON_EXISTENT: NrosRmwRet = -8;
/// Backend does not support loaned messages on this entity, or slot in use.
pub const NROS_RMW_RET_LOAN_NOT_SUPPORTED: NrosRmwRet = -9;
/// No data on a non-blocking receive (distinct from `TIMEOUT`).
pub const NROS_RMW_RET_NO_DATA: NrosRmwRet = -10;
/// Resource momentarily unavailable; caller should retry.
pub const NROS_RMW_RET_WOULD_BLOCK: NrosRmwRet = -11;
/// Caller buffer smaller than the data the backend wants to deliver.
pub const NROS_RMW_RET_BUFFER_TOO_SMALL: NrosRmwRet = -12;
/// Incoming message exceeded the backend's static capacity.
pub const NROS_RMW_RET_MESSAGE_TOO_LARGE: NrosRmwRet = -13;

/// Map a `TransportError` to the corresponding `nros_rmw_ret_t` code.
///
/// By-reference because `TransportError` carries a `String` on its
/// dynamic-diagnostic variant and is not `Copy`. The string itself is
/// dropped at the boundary — embedded RMW callers cannot afford a
/// thread-local error buffer.
pub fn ret_from_error(err: &TransportError) -> NrosRmwRet {
    match err {
        TransportError::Timeout => NROS_RMW_RET_TIMEOUT,
        TransportError::WouldBlock => NROS_RMW_RET_WOULD_BLOCK,
        TransportError::TooLarge => NROS_RMW_RET_MESSAGE_TOO_LARGE,
        TransportError::BufferTooSmall => NROS_RMW_RET_BUFFER_TOO_SMALL,
        TransportError::MessageTooLarge => NROS_RMW_RET_MESSAGE_TOO_LARGE,
        TransportError::InvalidArgument => NROS_RMW_RET_INVALID_ARGUMENT,
        TransportError::InvalidConfig => NROS_RMW_RET_INVALID_ARGUMENT,
        TransportError::Unsupported => NROS_RMW_RET_UNSUPPORTED,
        TransportError::BadAlloc => NROS_RMW_RET_BAD_ALLOC,
        TransportError::IncompatibleQos => NROS_RMW_RET_INCOMPATIBLE_QOS,
        TransportError::TopicNameInvalid => NROS_RMW_RET_TOPIC_NAME_INVALID,
        TransportError::NodeNameNonExistent => NROS_RMW_RET_NODE_NAME_NON_EXISTENT,
        TransportError::LoanNotSupported => NROS_RMW_RET_LOAN_NOT_SUPPORTED,
        TransportError::NoData => NROS_RMW_RET_NO_DATA,
        // Everything else collapses to NROS_RMW_RET_ERROR. Backends
        // that want fine-grained reporting should adopt the named
        // variants above (Phase 102.2 sweep).
        _ => NROS_RMW_RET_ERROR,
    }
}

/// Map a `nros_rmw_ret_t` returned by a C-side vtable function back to
/// a `TransportError` for the Rust caller. Inverse of `ret_from_error`
/// — used when `nros-rmw-cffi`'s `CffiSession` etc. receive a code
/// from the registered C backend.
///
/// `NROS_RMW_RET_OK` is mapped to `TransportError::Backend("ok")` as a
/// programming-error sentinel; callers should branch on the success
/// path before calling this. Unknown negative codes collapse to the
/// generic `TransportError::Backend("unknown rmw_ret_t")` so a future
/// constant added to the C header degrades gracefully on the Rust side.
pub fn error_from_ret(ret: NrosRmwRet) -> TransportError {
    match ret {
        NROS_RMW_RET_OK => TransportError::Backend("ok (logic error: positive ret_t at error site)"),
        NROS_RMW_RET_ERROR => TransportError::Backend("rmw_ret error"),
        NROS_RMW_RET_TIMEOUT => TransportError::Timeout,
        NROS_RMW_RET_BAD_ALLOC => TransportError::BadAlloc,
        NROS_RMW_RET_INVALID_ARGUMENT => TransportError::InvalidArgument,
        NROS_RMW_RET_UNSUPPORTED => TransportError::Unsupported,
        NROS_RMW_RET_INCOMPATIBLE_QOS => TransportError::IncompatibleQos,
        NROS_RMW_RET_TOPIC_NAME_INVALID => TransportError::TopicNameInvalid,
        NROS_RMW_RET_NODE_NAME_NON_EXISTENT => TransportError::NodeNameNonExistent,
        NROS_RMW_RET_LOAN_NOT_SUPPORTED => TransportError::LoanNotSupported,
        NROS_RMW_RET_NO_DATA => TransportError::NoData,
        NROS_RMW_RET_WOULD_BLOCK => TransportError::WouldBlock,
        NROS_RMW_RET_BUFFER_TOO_SMALL => TransportError::BufferTooSmall,
        NROS_RMW_RET_MESSAGE_TOO_LARGE => TransportError::MessageTooLarge,
        _ => TransportError::Backend("unknown rmw_ret_t"),
    }
}

// ============================================================================
// Phase 102.3 — typed entity structs (mirrors `<nros/rmw_entity.h>`)
// ============================================================================
//
// These structs are layout-compatible with the typed entity structs
// in the C header. They are introduced in 102.3 (header + Rust
// mirror) without changing the vtable signature; 102.4 will switch
// the vtable's `create_*` calls to use them as out-parameters and
// retire `CffiHandle` for those entities.

/// QoS values. Mirrors `nros_rmw_qos_t` from `<nros/rmw_entity.h>`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NrosRmwQos {
    /// Reliability policy: `0` = best-effort, `1` = reliable.
    pub reliability: u8,
    /// Durability policy: `0` = volatile, `1` = transient-local.
    pub durability: u8,
    /// History policy: `0` = keep-last, `1` = keep-all.
    pub history: u8,
    /// Reserved padding; must be zero.
    pub _pad0: u8,
    /// History depth (0–65 535).
    pub depth: u16,
    /// Reserved padding; must be zero.
    pub _pad1: u16,
}

/// Lending capabilities. Mirrors `nros_rmw_loan_caps_t` from
/// `<nros/rmw_entity.h>`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NrosRmwLoanCaps {
    /// Bit 0: `supports_cdr_loan`. Bit 1: `supports_typed_loan`.
    /// Remaining bits reserved.
    pub bits: u8,
}

impl NrosRmwLoanCaps {
    /// `true` iff `supports_cdr_loan` is set.
    #[inline]
    pub fn supports_cdr_loan(&self) -> bool {
        self.bits & 0b0000_0001 != 0
    }

    /// `true` iff `supports_typed_loan` is set.
    #[inline]
    pub fn supports_typed_loan(&self) -> bool {
        self.bits & 0b0000_0010 != 0
    }
}

/// Per-process RMW session. Mirrors `nros_rmw_session_t`.
#[repr(C)]
pub struct NrosRmwSession {
    /// Borrowed; outlives the session.
    pub node_name: *const u8,
    /// Borrowed; outlives the session.
    pub namespace_: *const u8,
    /// Opaque backend state. NULL when uninitialised.
    pub backend_data: *mut c_void,
}

/// Publisher entity. Mirrors `nros_rmw_publisher_t`.
#[repr(C)]
pub struct NrosRmwPublisher {
    /// Borrowed; outlives the publisher.
    pub topic_name: *const u8,
    /// Borrowed; outlives the publisher.
    pub type_name: *const u8,
    pub qos: NrosRmwQos,
    pub loan_caps: NrosRmwLoanCaps,
    /// Opaque backend state. NULL when creation failed.
    pub backend_data: *mut c_void,
}

/// Subscriber entity. Mirrors `nros_rmw_subscriber_t`.
#[repr(C)]
pub struct NrosRmwSubscriber {
    /// Borrowed; outlives the subscriber.
    pub topic_name: *const u8,
    /// Borrowed; outlives the subscriber.
    pub type_name: *const u8,
    pub qos: NrosRmwQos,
    pub loan_caps: NrosRmwLoanCaps,
    /// Opaque backend state. NULL when creation failed.
    pub backend_data: *mut c_void,
}

/// Service-server entity. Mirrors `nros_rmw_service_server_t`.
#[repr(C)]
pub struct NrosRmwServiceServer {
    /// Borrowed; outlives the server.
    pub service_name: *const u8,
    /// Borrowed; outlives the server.
    pub type_name: *const u8,
    /// Opaque backend state. NULL when creation failed.
    pub backend_data: *mut c_void,
}

/// Service-client entity. Mirrors `nros_rmw_service_client_t`.
#[repr(C)]
pub struct NrosRmwServiceClient {
    /// Borrowed; outlives the client.
    pub service_name: *const u8,
    /// Borrowed; outlives the client.
    pub type_name: *const u8,
    /// Opaque backend state. NULL when creation failed.
    pub backend_data: *mut c_void,
}

impl From<QosSettings> for NrosRmwQos {
    fn from(qos: QosSettings) -> Self {
        Self {
            reliability: match qos.reliability {
                QosReliabilityPolicy::BestEffort => 0,
                QosReliabilityPolicy::Reliable => 1,
            },
            durability: match qos.durability {
                QosDurabilityPolicy::Volatile => 0,
                QosDurabilityPolicy::TransientLocal => 1,
            },
            history: match qos.history {
                QosHistoryPolicy::KeepLast => 0,
                QosHistoryPolicy::KeepAll => 1,
            },
            _pad0: 0,
            // QosSettings::depth is u32; clamp to u16 max. Embedded
            // ROS queue depths are typically 1–100; oversize values
            // are saturated at 65 535 rather than wrapped.
            depth: qos.depth.min(u16::MAX as u32) as u16,
            _pad1: 0,
        }
    }
}

// ============================================================================
// Vtable type (mirrors C header)
// ============================================================================

/// Opaque handle passed through the C vtable.
pub type CffiHandle = *mut c_void;

/// QoS settings in C-compatible layout.
#[repr(C)]
pub struct CffiQos {
    pub reliability: u8,
    pub durability: u8,
    pub history: u8,
    pub depth: u32,
}

impl From<QosSettings> for CffiQos {
    fn from(qos: QosSettings) -> Self {
        Self {
            reliability: match qos.reliability {
                QosReliabilityPolicy::BestEffort => 0,
                QosReliabilityPolicy::Reliable => 1,
            },
            durability: match qos.durability {
                QosDurabilityPolicy::Volatile => 0,
                QosDurabilityPolicy::TransientLocal => 1,
            },
            history: match qos.history {
                QosHistoryPolicy::KeepLast => 0,
                QosHistoryPolicy::KeepAll => 1,
            },
            depth: qos.depth,
        }
    }
}

/// C function table for an RMW backend.
///
/// This struct mirrors `nros_rmw_vtable_t` from the C header.
#[repr(C)]
pub struct NrosRmwVtable {
    // Session lifecycle
    pub open: unsafe extern "C" fn(
        locator: *const u8,
        mode: u8,
        domain_id: u32,
        node_name: *const u8,
    ) -> CffiHandle,
    pub close: unsafe extern "C" fn(session: CffiHandle) -> i32,
    pub drive_io: unsafe extern "C" fn(session: CffiHandle, timeout_ms: i32) -> i32,

    // Publisher
    pub create_publisher: unsafe extern "C" fn(
        session: CffiHandle,
        topic_name: *const u8,
        type_name: *const u8,
        type_hash: *const u8,
        domain_id: u32,
        qos: *const CffiQos,
    ) -> CffiHandle,
    pub destroy_publisher: unsafe extern "C" fn(publisher: CffiHandle),
    pub publish_raw:
        unsafe extern "C" fn(publisher: CffiHandle, data: *const u8, len: usize) -> i32,

    // Subscriber
    pub create_subscriber: unsafe extern "C" fn(
        session: CffiHandle,
        topic_name: *const u8,
        type_name: *const u8,
        type_hash: *const u8,
        domain_id: u32,
        qos: *const CffiQos,
    ) -> CffiHandle,
    pub destroy_subscriber: unsafe extern "C" fn(subscriber: CffiHandle),
    pub try_recv_raw:
        unsafe extern "C" fn(subscriber: CffiHandle, buf: *mut u8, buf_len: usize) -> i32,
    pub has_data: unsafe extern "C" fn(subscriber: CffiHandle) -> i32,

    // Service Server
    pub create_service_server: unsafe extern "C" fn(
        session: CffiHandle,
        service_name: *const u8,
        type_name: *const u8,
        type_hash: *const u8,
        domain_id: u32,
    ) -> CffiHandle,
    pub destroy_service_server: unsafe extern "C" fn(server: CffiHandle),
    pub try_recv_request: unsafe extern "C" fn(
        server: CffiHandle,
        buf: *mut u8,
        buf_len: usize,
        seq_out: *mut i64,
    ) -> i32,
    pub has_request: unsafe extern "C" fn(server: CffiHandle) -> i32,
    pub send_reply:
        unsafe extern "C" fn(server: CffiHandle, seq: i64, data: *const u8, len: usize) -> i32,

    // Service Client
    pub create_service_client: unsafe extern "C" fn(
        session: CffiHandle,
        service_name: *const u8,
        type_name: *const u8,
        type_hash: *const u8,
        domain_id: u32,
    ) -> CffiHandle,
    pub destroy_service_client: unsafe extern "C" fn(client: CffiHandle),
    pub call_raw: unsafe extern "C" fn(
        client: CffiHandle,
        request: *const u8,
        req_len: usize,
        reply_buf: *mut u8,
        reply_buf_len: usize,
    ) -> i32,
}

// ============================================================================
// Registration
// ============================================================================

static VTABLE: AtomicPtr<NrosRmwVtable> = AtomicPtr::new(core::ptr::null_mut());

/// Register a custom RMW backend vtable.
///
/// # Safety
///
/// The vtable pointer must remain valid for the lifetime of the program.
/// All function pointers in the vtable must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_rmw_cffi_register(vtable: *const NrosRmwVtable) -> i32 {
    VTABLE.store(vtable as *mut NrosRmwVtable, Ordering::Release);
    0
}

fn get_vtable() -> Result<&'static NrosRmwVtable, TransportError> {
    let ptr = VTABLE.load(Ordering::Acquire);
    if ptr.is_null() {
        // No vtable registered — caller forgot nros_rmw_cffi_register.
        return Err(TransportError::InvalidArgument);
    }
    // SAFETY: Registration ensures the pointer is valid and 'static.
    Ok(unsafe { &*ptr })
}

// ============================================================================
// Helper: null-terminated string on the stack
// ============================================================================

/// Write a Rust `&str` as a null-terminated byte sequence into a fixed buffer.
/// Returns a pointer to the buffer start.
fn to_c_str<const N: usize>(s: &str, buf: &mut [u8; N]) -> *const u8 {
    let len = s.len().min(N - 1);
    buf[..len].copy_from_slice(&s.as_bytes()[..len]);
    buf[len] = 0;
    buf.as_ptr()
}

// ============================================================================
// CffiSession
// ============================================================================

/// Session backed by a C vtable.
pub struct CffiSession {
    vtable: &'static NrosRmwVtable,
    handle: CffiHandle,
}

impl CffiSession {
    /// Open a new session via the registered vtable.
    pub fn open(
        locator: &str,
        mode: u8,
        domain_id: u32,
        node_name: &str,
    ) -> Result<Self, TransportError> {
        let vtable = get_vtable()?;
        let mut loc_buf = [0u8; 256];
        let loc_ptr = to_c_str(locator, &mut loc_buf);
        let mut name_buf = [0u8; 128];
        let name_ptr = to_c_str(node_name, &mut name_buf);

        let handle = unsafe { (vtable.open)(loc_ptr, mode, domain_id, name_ptr) };
        if handle.is_null() {
            return Err(TransportError::ConnectionFailed);
        }
        Ok(Self { vtable, handle })
    }
}

impl Session for CffiSession {
    type Error = TransportError;
    type PublisherHandle = CffiPublisher;
    type SubscriberHandle = CffiSubscriber;
    type ServiceServerHandle = CffiServiceServer;
    type ServiceClientHandle = CffiServiceClient;

    fn create_publisher(
        &mut self,
        topic: &TopicInfo,
        qos: QosSettings,
    ) -> Result<CffiPublisher, TransportError> {
        let mut name_buf = [0u8; 256];
        let name_ptr = to_c_str(topic.name, &mut name_buf);
        let mut type_buf = [0u8; 256];
        let type_ptr = to_c_str(topic.type_name, &mut type_buf);
        let mut hash_buf = [0u8; 128];
        let hash_ptr = to_c_str(topic.type_hash, &mut hash_buf);
        let cffi_qos = CffiQos::from(qos);

        let handle = unsafe {
            (self.vtable.create_publisher)(
                self.handle,
                name_ptr,
                type_ptr,
                hash_ptr,
                topic.domain_id,
                &cffi_qos,
            )
        };
        if handle.is_null() {
            return Err(TransportError::PublisherCreationFailed);
        }
        Ok(CffiPublisher {
            vtable: self.vtable,
            handle,
        })
    }

    fn create_subscriber(
        &mut self,
        topic: &TopicInfo,
        qos: QosSettings,
    ) -> Result<CffiSubscriber, TransportError> {
        let mut name_buf = [0u8; 256];
        let name_ptr = to_c_str(topic.name, &mut name_buf);
        let mut type_buf = [0u8; 256];
        let type_ptr = to_c_str(topic.type_name, &mut type_buf);
        let mut hash_buf = [0u8; 128];
        let hash_ptr = to_c_str(topic.type_hash, &mut hash_buf);
        let cffi_qos = CffiQos::from(qos);

        let handle = unsafe {
            (self.vtable.create_subscriber)(
                self.handle,
                name_ptr,
                type_ptr,
                hash_ptr,
                topic.domain_id,
                &cffi_qos,
            )
        };
        if handle.is_null() {
            return Err(TransportError::SubscriberCreationFailed);
        }
        Ok(CffiSubscriber {
            vtable: self.vtable,
            handle,
        })
    }

    fn create_service_server(
        &mut self,
        service: &ServiceInfo,
    ) -> Result<CffiServiceServer, TransportError> {
        let mut name_buf = [0u8; 256];
        let name_ptr = to_c_str(service.name, &mut name_buf);
        let mut type_buf = [0u8; 256];
        let type_ptr = to_c_str(service.type_name, &mut type_buf);
        let mut hash_buf = [0u8; 128];
        let hash_ptr = to_c_str(service.type_hash, &mut hash_buf);

        let handle = unsafe {
            (self.vtable.create_service_server)(
                self.handle,
                name_ptr,
                type_ptr,
                hash_ptr,
                service.domain_id,
            )
        };
        if handle.is_null() {
            return Err(TransportError::ServiceServerCreationFailed);
        }
        Ok(CffiServiceServer {
            vtable: self.vtable,
            handle,
        })
    }

    fn create_service_client(
        &mut self,
        service: &ServiceInfo,
    ) -> Result<CffiServiceClient, TransportError> {
        let mut name_buf = [0u8; 256];
        let name_ptr = to_c_str(service.name, &mut name_buf);
        let mut type_buf = [0u8; 256];
        let type_ptr = to_c_str(service.type_name, &mut type_buf);
        let mut hash_buf = [0u8; 128];
        let hash_ptr = to_c_str(service.type_hash, &mut hash_buf);

        let handle = unsafe {
            (self.vtable.create_service_client)(
                self.handle,
                name_ptr,
                type_ptr,
                hash_ptr,
                service.domain_id,
            )
        };
        if handle.is_null() {
            return Err(TransportError::ServiceClientCreationFailed);
        }
        Ok(CffiServiceClient {
            vtable: self.vtable,
            handle,
            pending_request: [0u8; 4096],
            pending_len: 0,
        })
    }

    fn close(&mut self) -> Result<(), TransportError> {
        let rc = unsafe { (self.vtable.close)(self.handle) };
        if rc < 0 {
            return Err(TransportError::Disconnected);
        }
        self.handle = core::ptr::null_mut();
        Ok(())
    }

    fn drive_io(&mut self, timeout_ms: i32) -> Result<(), TransportError> {
        let rc = unsafe { (self.vtable.drive_io)(self.handle, timeout_ms) };
        if rc < 0 {
            return Err(TransportError::PollFailed);
        }
        Ok(())
    }
}

impl Drop for CffiSession {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe { (self.vtable.close)(self.handle) };
        }
    }
}

// ============================================================================
// CffiPublisher
// ============================================================================

/// Publisher backed by a C vtable.
pub struct CffiPublisher {
    vtable: &'static NrosRmwVtable,
    handle: CffiHandle,
}

impl Publisher for CffiPublisher {
    type Error = TransportError;

    fn publish_raw(&self, data: &[u8]) -> Result<(), TransportError> {
        let rc = unsafe { (self.vtable.publish_raw)(self.handle, data.as_ptr(), data.len()) };
        if rc < 0 {
            return Err(TransportError::PublishFailed);
        }
        Ok(())
    }

    fn buffer_error(&self) -> TransportError {
        TransportError::BufferTooSmall
    }

    fn serialization_error(&self) -> TransportError {
        TransportError::SerializationError
    }
}

impl Drop for CffiPublisher {
    fn drop(&mut self) {
        unsafe { (self.vtable.destroy_publisher)(self.handle) };
    }
}

// ============================================================================
// CffiSubscriber
// ============================================================================

/// Subscriber backed by a C vtable.
pub struct CffiSubscriber {
    vtable: &'static NrosRmwVtable,
    handle: CffiHandle,
}

impl nros_rmw::Subscriber for CffiSubscriber {
    type Error = TransportError;

    fn has_data(&self) -> bool {
        let rc = unsafe { (self.vtable.has_data)(self.handle) };
        rc > 0
    }

    fn try_recv_raw(&mut self, buf: &mut [u8]) -> Result<Option<usize>, TransportError> {
        let rc = unsafe { (self.vtable.try_recv_raw)(self.handle, buf.as_mut_ptr(), buf.len()) };
        if rc < 0 {
            return Err(TransportError::DeserializationError);
        }
        if rc == 0 {
            return Ok(None);
        }
        Ok(Some(rc as usize))
    }

    fn deserialization_error(&self) -> TransportError {
        TransportError::DeserializationError
    }
}

impl Drop for CffiSubscriber {
    fn drop(&mut self) {
        unsafe { (self.vtable.destroy_subscriber)(self.handle) };
    }
}

// ============================================================================
// CffiServiceServer
// ============================================================================

/// Service server backed by a C vtable.
pub struct CffiServiceServer {
    vtable: &'static NrosRmwVtable,
    handle: CffiHandle,
}

impl ServiceServerTrait for CffiServiceServer {
    type Error = TransportError;

    fn has_request(&self) -> bool {
        let rc = unsafe { (self.vtable.has_request)(self.handle) };
        rc > 0
    }

    fn try_recv_request<'a>(
        &mut self,
        buf: &'a mut [u8],
    ) -> Result<Option<ServiceRequest<'a>>, TransportError> {
        let mut seq: i64 = 0;
        let rc = unsafe {
            (self.vtable.try_recv_request)(self.handle, buf.as_mut_ptr(), buf.len(), &mut seq)
        };
        if rc < 0 {
            return Err(TransportError::ServiceRequestFailed);
        }
        if rc == 0 {
            return Ok(None);
        }
        let len = rc as usize;
        Ok(Some(ServiceRequest {
            data: &buf[..len],
            sequence_number: seq,
        }))
    }

    fn send_reply(&mut self, sequence_number: i64, data: &[u8]) -> Result<(), TransportError> {
        let rc = unsafe {
            (self.vtable.send_reply)(self.handle, sequence_number, data.as_ptr(), data.len())
        };
        if rc < 0 {
            return Err(TransportError::ServiceReplyFailed);
        }
        Ok(())
    }
}

impl Drop for CffiServiceServer {
    fn drop(&mut self) {
        unsafe { (self.vtable.destroy_service_server)(self.handle) };
    }
}

// ============================================================================
// CffiServiceClient
// ============================================================================

/// Service client backed by a C vtable.
pub struct CffiServiceClient {
    vtable: &'static NrosRmwVtable,
    handle: CffiHandle,
    /// Stored request for blocking fallback in `try_recv_reply_raw`
    pending_request: [u8; 4096],
    /// Length of stored pending request (0 = no pending request)
    pending_len: usize,
}

impl ServiceClientTrait for CffiServiceClient {
    type Error = TransportError;

    #[allow(deprecated)]
    fn call_raw(&mut self, request: &[u8], reply_buf: &mut [u8]) -> Result<usize, TransportError> {
        let rc = unsafe {
            (self.vtable.call_raw)(
                self.handle,
                request.as_ptr(),
                request.len(),
                reply_buf.as_mut_ptr(),
                reply_buf.len(),
            )
        };
        if rc < 0 {
            return Err(TransportError::ServiceRequestFailed);
        }
        Ok(rc as usize)
    }

    fn send_request_raw(&mut self, request: &[u8]) -> Result<(), TransportError> {
        if request.len() > self.pending_request.len() {
            return Err(TransportError::BufferTooSmall);
        }
        self.pending_request[..request.len()].copy_from_slice(request);
        self.pending_len = request.len();
        Ok(())
    }

    fn try_recv_reply_raw(
        &mut self,
        reply_buf: &mut [u8],
    ) -> Result<Option<usize>, TransportError> {
        if self.pending_len == 0 {
            return Ok(None);
        }
        // Blocking fallback: copy request to stack, then call_raw
        let mut req_copy = [0u8; 4096];
        let req_len = self.pending_len;
        req_copy[..req_len].copy_from_slice(&self.pending_request[..req_len]);
        self.pending_len = 0;
        #[allow(deprecated)]
        let len = self.call_raw(&req_copy[..req_len], reply_buf)?;
        Ok(Some(len))
    }
}

impl Drop for CffiServiceClient {
    fn drop(&mut self) {
        unsafe { (self.vtable.destroy_service_client)(self.handle) };
    }
}

// ============================================================================
// Factory
// ============================================================================

/// RMW factory for the C function table backend.
#[derive(Default)]
pub struct CffiRmw;

impl nros_rmw::Rmw for CffiRmw {
    type Session = CffiSession;
    type Error = TransportError;

    fn open(self, config: &nros_rmw::RmwConfig) -> Result<CffiSession, TransportError> {
        let mode = match config.mode {
            nros_rmw::SessionMode::Client => 0u8,
            nros_rmw::SessionMode::Peer => 1u8,
        };
        CffiSession::open(config.locator, mode, config.domain_id, config.node_name)
    }
}
