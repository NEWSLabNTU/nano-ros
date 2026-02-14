//! Low-level FFI bindings to Micro-XRCE-DDS-Client v3.0.1 and Micro-CDR v2.0.2.
//!
//! This crate compiles the XRCE-DDS Client C library with custom transport
//! support and exposes its API to Rust. No bindgen — all types are
//! hand-written from the public C headers.

#![no_std]
#![allow(non_camel_case_types, non_upper_case_globals)]

use core::ffi::{c_char, c_int, c_void};

// ============================================================================
// Opaque blob sizes for uxrSession and uxrCustomTransport
//
// These must be >= sizeof(C struct). Verified at compile time by
// _Static_assert in build.rs-generated size_check.c.
// ============================================================================

/// Size of the opaque Rust blob for `uxrSession`.
/// Actual C size: ~328 bytes (x86_64), ~224 bytes (ARM32).
pub const UXR_SESSION_SIZE: usize = 512;

/// Size of the opaque Rust blob for `uxrCustomTransport`.
/// Actual C size: ~704 bytes (x86_64), ~650 bytes (ARM32).
/// Includes the 512-byte MTU buffer, framing I/O, and callback pointers.
pub const UXR_CUSTOM_TRANSPORT_SIZE: usize = 768;

// ============================================================================
// Entity type IDs (from object_id.h)
// ============================================================================

pub const UXR_INVALID_ID: u8 = 0x00;
pub const UXR_PARTICIPANT_ID: u8 = 0x01;
pub const UXR_TOPIC_ID: u8 = 0x02;
pub const UXR_PUBLISHER_ID: u8 = 0x03;
pub const UXR_SUBSCRIBER_ID: u8 = 0x04;
pub const UXR_DATAWRITER_ID: u8 = 0x05;
pub const UXR_DATAREADER_ID: u8 = 0x06;
pub const UXR_REQUESTER_ID: u8 = 0x07;
pub const UXR_REPLIER_ID: u8 = 0x08;

// ============================================================================
// Status codes (from session_info.h)
// ============================================================================

pub const UXR_STATUS_OK: u8 = 0x00;
pub const UXR_STATUS_OK_MATCHED: u8 = 0x01;
pub const UXR_STATUS_ERR_DDS_ERROR: u8 = 0x80;
pub const UXR_STATUS_ERR_MISMATCH: u8 = 0x81;
pub const UXR_STATUS_ERR_ALREADY_EXISTS: u8 = 0x82;
pub const UXR_STATUS_ERR_DENIED: u8 = 0x83;
pub const UXR_STATUS_ERR_UNKNOWN_REFERENCE: u8 = 0x84;
pub const UXR_STATUS_ERR_INVALID_DATA: u8 = 0x85;
pub const UXR_STATUS_ERR_INCOMPATIBLE: u8 = 0x86;
pub const UXR_STATUS_ERR_RESOURCES: u8 = 0x87;
pub const UXR_STATUS_NONE: u8 = 0xFF;

// ============================================================================
// Creation mode flags (from session_info.h)
// ============================================================================

pub const UXR_REUSE: u8 = 0x01 << 1;
pub const UXR_REPLACE: u8 = 0x01 << 2;

// ============================================================================
// Request IDs
// ============================================================================

pub const UXR_INVALID_REQUEST_ID: u16 = 0;

// ============================================================================
// Delivery control limits (from read_access.h)
// ============================================================================

pub const UXR_MAX_SAMPLES_UNLIMITED: u16 = 0xFFFF;
pub const UXR_MAX_ELAPSED_TIME_UNLIMITED: u16 = 0x0000;
pub const UXR_MAX_BYTES_PER_SECOND_UNLIMITED: u16 = 0x0000;

// ============================================================================
// Stream types (from stream_id.h)
// ============================================================================

pub const UXR_NONE_STREAM: u8 = 0;
pub const UXR_BEST_EFFORT_STREAM: u8 = 1;
pub const UXR_RELIABLE_STREAM: u8 = 2;

// ============================================================================
// Stream directions (from stream_id.h)
// ============================================================================

pub const UXR_INPUT_STREAM: u8 = 0;
pub const UXR_OUTPUT_STREAM: u8 = 1;

// ============================================================================
// Timeout
// ============================================================================

pub const UXR_TIMEOUT_INF: c_int = -1;

// ============================================================================
// Transparent #[repr(C)] types
// ============================================================================

/// Entity identifier (object_id.h).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct uxrObjectId {
    pub id: u16,
    pub type_: u8,
}

/// Stream identifier (stream_id.h).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct uxrStreamId {
    pub raw: u8,
    pub index: u8,
    pub type_: u8,
    pub direction: u8,
}

/// QoS configuration for entity creation (create_entities_bin.h).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct uxrQoS_t {
    pub durability: u32,
    pub reliability: u32,
    pub history: u32,
    pub depth: u16,
}

/// Subscription delivery control (read_access.h).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct uxrDeliveryControl {
    pub max_samples: u16,
    pub max_elapsed_time: u16,
    pub max_bytes_per_second: u16,
    pub min_pace_period: u16,
}

/// Micro-CDR serialization buffer (microcdr.h).
///
/// Fields match the C struct exactly. The `on_full_buffer` callback
/// and `args` pointer are included for completeness but typically
/// unused by our wrapper.
#[repr(C)]
pub struct ucdrBuffer {
    pub init: *mut u8,
    pub final_: *mut u8,
    pub iterator: *mut u8,
    pub origin: usize,
    pub offset: usize,
    pub endianness: u8,
    pub last_data_size: u8,
    pub error: bool,
    pub on_full_buffer:
        Option<unsafe extern "C" fn(buffer: *mut ucdrBuffer, args: *mut c_void) -> bool>,
    pub args: *mut c_void,
}

// ============================================================================
// SampleIdentity (xrce_types.h) — used for service request/reply correlation
// ============================================================================

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct GuidPrefix_t {
    pub data: [u8; 12],
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct EntityId_t {
    pub entity_key: [u8; 3],
    pub entity_kind: u8,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct GUID_t {
    pub guid_prefix: GuidPrefix_t,
    pub entity_id: EntityId_t,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SequenceNumber_t {
    pub high: i32,
    pub low: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SampleIdentity {
    pub writer_guid: GUID_t,
    pub sequence_number: SequenceNumber_t,
}

// ============================================================================
// Opaque types (pointer-only or blob-based)
// ============================================================================

/// Opaque session structure. The actual C struct size depends on config.h
/// defines (stream counts, multithread, etc.). We use a fixed-size byte
/// array verified at compile time to be >= sizeof(uxrSession).
#[repr(C, align(8))]
pub struct uxrSession {
    _opaque: [u8; UXR_SESSION_SIZE],
}

/// Opaque custom transport structure. Same approach as uxrSession.
#[repr(C, align(8))]
pub struct uxrCustomTransport {
    _opaque: [u8; UXR_CUSTOM_TRANSPORT_SIZE],
}

/// Communication abstraction (used only as pointer).
#[repr(C)]
pub struct uxrCommunication {
    _opaque: [u8; 0],
}

// ============================================================================
// Callback function pointer types
// ============================================================================

/// Status callback (session.h).
pub type uxrOnStatusFunc = Option<
    unsafe extern "C" fn(
        session: *mut uxrSession,
        object_id: uxrObjectId,
        request_id: u16,
        status: u8,
        args: *mut c_void,
    ),
>;

/// Topic data callback (session.h).
pub type uxrOnTopicFunc = Option<
    unsafe extern "C" fn(
        session: *mut uxrSession,
        object_id: uxrObjectId,
        request_id: u16,
        stream_id: uxrStreamId,
        ub: *mut ucdrBuffer,
        length: u16,
        args: *mut c_void,
    ),
>;

/// Service request callback (session.h).
pub type uxrOnRequestFunc = Option<
    unsafe extern "C" fn(
        session: *mut uxrSession,
        object_id: uxrObjectId,
        request_id: u16,
        sample_id: *mut SampleIdentity,
        ub: *mut ucdrBuffer,
        length: u16,
        args: *mut c_void,
    ),
>;

/// Service reply callback (session.h).
pub type uxrOnReplyFunc = Option<
    unsafe extern "C" fn(
        session: *mut uxrSession,
        object_id: uxrObjectId,
        request_id: u16,
        reply_id: u16,
        ub: *mut ucdrBuffer,
        length: u16,
        args: *mut c_void,
    ),
>;

/// Custom transport: open callback.
pub type open_custom_func =
    Option<unsafe extern "C" fn(transport: *mut uxrCustomTransport) -> bool>;

/// Custom transport: close callback.
pub type close_custom_func =
    Option<unsafe extern "C" fn(transport: *mut uxrCustomTransport) -> bool>;

/// Custom transport: write callback.
pub type write_custom_func = Option<
    unsafe extern "C" fn(
        transport: *mut uxrCustomTransport,
        buffer: *const u8,
        length: usize,
        error_code: *mut u8,
    ) -> usize,
>;

/// Custom transport: read callback.
pub type read_custom_func = Option<
    unsafe extern "C" fn(
        transport: *mut uxrCustomTransport,
        buffer: *mut u8,
        length: usize,
        timeout: c_int,
        error_code: *mut u8,
    ) -> usize,
>;

// ============================================================================
// QoS enum values (create_entities_bin.h)
// ============================================================================

// uxrQoSDurability
pub const UXR_DURABILITY_TRANSIENT_LOCAL: u32 = 0;
pub const UXR_DURABILITY_TRANSIENT: u32 = 1;
pub const UXR_DURABILITY_VOLATILE: u32 = 2;
pub const UXR_DURABILITY_PERSISTENT: u32 = 3;

// uxrQoSReliability
pub const UXR_RELIABILITY_RELIABLE: u32 = 0;
pub const UXR_RELIABILITY_BEST_EFFORT: u32 = 1;

// uxrQoSHistory
pub const UXR_HISTORY_KEEP_LAST: u32 = 0;
pub const UXR_HISTORY_KEEP_ALL: u32 = 1;

// ============================================================================
// Extern C functions
// ============================================================================

unsafe extern "C" {
    // --- Helpers (object_id.h, stream_id.h) ---

    pub fn uxr_object_id(id: u16, type_: u8) -> uxrObjectId;
    pub fn uxr_stream_id(index: u8, type_: u8, direction: u8) -> uxrStreamId;
    pub fn uxr_stream_id_from_raw(stream_id_raw: u8, direction: u8) -> uxrStreamId;

    // --- Custom transport (custom_transport.h) ---

    pub fn uxr_set_custom_transport_callbacks(
        transport: *mut uxrCustomTransport,
        framing: bool,
        open: open_custom_func,
        close: close_custom_func,
        write: write_custom_func,
        read: read_custom_func,
    );

    pub fn uxr_init_custom_transport(transport: *mut uxrCustomTransport, args: *mut c_void)
    -> bool;

    pub fn uxr_close_custom_transport(transport: *mut uxrCustomTransport) -> bool;

    // --- Session lifecycle (session.h) ---

    pub fn uxr_init_session(session: *mut uxrSession, comm: *mut uxrCommunication, key: u32);

    pub fn uxr_create_session(session: *mut uxrSession) -> bool;

    pub fn uxr_create_session_retries(session: *mut uxrSession, retries: usize) -> bool;

    pub fn uxr_delete_session(session: *mut uxrSession) -> bool;

    pub fn uxr_delete_session_retries(session: *mut uxrSession, retries: usize) -> bool;

    // --- Session callbacks (session.h) ---

    pub fn uxr_set_status_callback(
        session: *mut uxrSession,
        on_status_func: uxrOnStatusFunc,
        args: *mut c_void,
    );

    pub fn uxr_set_topic_callback(
        session: *mut uxrSession,
        on_topic_func: uxrOnTopicFunc,
        args: *mut c_void,
    );

    pub fn uxr_set_request_callback(
        session: *mut uxrSession,
        on_request_func: uxrOnRequestFunc,
        args: *mut c_void,
    );

    pub fn uxr_set_reply_callback(
        session: *mut uxrSession,
        on_reply_func: uxrOnReplyFunc,
        args: *mut c_void,
    );

    // --- Streams (session.h) ---

    pub fn uxr_create_output_best_effort_stream(
        session: *mut uxrSession,
        buffer: *mut u8,
        size: usize,
    ) -> uxrStreamId;

    pub fn uxr_create_output_reliable_stream(
        session: *mut uxrSession,
        buffer: *mut u8,
        size: usize,
        history: u16,
    ) -> uxrStreamId;

    pub fn uxr_create_input_best_effort_stream(session: *mut uxrSession) -> uxrStreamId;

    pub fn uxr_create_input_reliable_stream(
        session: *mut uxrSession,
        buffer: *mut u8,
        size: usize,
        history: u16,
    ) -> uxrStreamId;

    pub fn uxr_flash_output_streams(session: *mut uxrSession);

    // --- Session run (session.h) ---

    pub fn uxr_run_session_time(session: *mut uxrSession, timeout: c_int) -> bool;

    pub fn uxr_run_session_timeout(session: *mut uxrSession, timeout: c_int) -> bool;

    pub fn uxr_run_session_until_data(session: *mut uxrSession, timeout: c_int) -> bool;

    pub fn uxr_run_session_until_timeout(session: *mut uxrSession, timeout: c_int) -> bool;

    pub fn uxr_run_session_until_confirm_delivery(session: *mut uxrSession, timeout: c_int)
    -> bool;

    pub fn uxr_run_session_until_all_status(
        session: *mut uxrSession,
        timeout: c_int,
        request_list: *const u16,
        status_list: *mut u8,
        list_size: usize,
    ) -> bool;

    pub fn uxr_run_session_until_one_status(
        session: *mut uxrSession,
        timeout: c_int,
        request_list: *const u16,
        status_list: *mut u8,
        list_size: usize,
    ) -> bool;

    // --- Entity creation — binary (create_entities_bin.h) ---

    pub fn uxr_buffer_create_participant_bin(
        session: *mut uxrSession,
        stream_id: uxrStreamId,
        object_id: uxrObjectId,
        domain_id: u16,
        participant_name: *const c_char,
        mode: u8,
    ) -> u16;

    pub fn uxr_buffer_create_topic_bin(
        session: *mut uxrSession,
        stream_id: uxrStreamId,
        object_id: uxrObjectId,
        participant_id: uxrObjectId,
        topic_name: *const c_char,
        type_name: *const c_char,
        mode: u8,
    ) -> u16;

    pub fn uxr_buffer_create_publisher_bin(
        session: *mut uxrSession,
        stream_id: uxrStreamId,
        object_id: uxrObjectId,
        participant_id: uxrObjectId,
        mode: u8,
    ) -> u16;

    pub fn uxr_buffer_create_subscriber_bin(
        session: *mut uxrSession,
        stream_id: uxrStreamId,
        object_id: uxrObjectId,
        participant_id: uxrObjectId,
        mode: u8,
    ) -> u16;

    pub fn uxr_buffer_create_datawriter_bin(
        session: *mut uxrSession,
        stream_id: uxrStreamId,
        object_id: uxrObjectId,
        publisher_id: uxrObjectId,
        topic_id: uxrObjectId,
        qos: uxrQoS_t,
        mode: u8,
    ) -> u16;

    pub fn uxr_buffer_create_datareader_bin(
        session: *mut uxrSession,
        stream_id: uxrStreamId,
        object_id: uxrObjectId,
        subscriber_id: uxrObjectId,
        topic_id: uxrObjectId,
        qos: uxrQoS_t,
        mode: u8,
    ) -> u16;

    pub fn uxr_buffer_create_requester_bin(
        session: *mut uxrSession,
        stream_id: uxrStreamId,
        object_id: uxrObjectId,
        participant_id: uxrObjectId,
        service_name: *const c_char,
        request_type: *const c_char,
        reply_type: *const c_char,
        request_topic_name: *const c_char,
        reply_topic_name: *const c_char,
        qos: uxrQoS_t,
        mode: u8,
    ) -> u16;

    pub fn uxr_buffer_create_replier_bin(
        session: *mut uxrSession,
        stream_id: uxrStreamId,
        object_id: uxrObjectId,
        participant_id: uxrObjectId,
        service_name: *const c_char,
        request_type: *const c_char,
        reply_type: *const c_char,
        request_topic_name: *const c_char,
        reply_topic_name: *const c_char,
        qos: uxrQoS_t,
        mode: u8,
    ) -> u16;

    // --- Entity deletion (common_create_entities.h) ---

    pub fn uxr_buffer_delete_entity(
        session: *mut uxrSession,
        stream_id: uxrStreamId,
        object_id: uxrObjectId,
    ) -> u16;

    // --- Write access (write_access.h) ---

    pub fn uxr_buffer_topic(
        session: *mut uxrSession,
        stream_id: uxrStreamId,
        datawriter_id: uxrObjectId,
        buffer: *mut u8,
        len: usize,
    ) -> u16;

    pub fn uxr_prepare_output_stream(
        session: *mut uxrSession,
        stream_id: uxrStreamId,
        entity_id: uxrObjectId,
        ub: *mut ucdrBuffer,
        len: u32,
    ) -> u16;

    pub fn uxr_buffer_request(
        session: *mut uxrSession,
        stream_id: uxrStreamId,
        requester_id: uxrObjectId,
        buffer: *mut u8,
        len: usize,
    ) -> u16;

    pub fn uxr_buffer_reply(
        session: *mut uxrSession,
        stream_id: uxrStreamId,
        replier_id: uxrObjectId,
        sample_id: *mut SampleIdentity,
        buffer: *mut u8,
        len: usize,
    ) -> u16;

    // --- Read access (read_access.h) ---

    pub fn uxr_buffer_request_data(
        session: *mut uxrSession,
        stream_id: uxrStreamId,
        datareader_id: uxrObjectId,
        data_stream_id: uxrStreamId,
        delivery_control: *const uxrDeliveryControl,
    ) -> u16;

    pub fn uxr_buffer_cancel_data(
        session: *mut uxrSession,
        stream_id: uxrStreamId,
        datareader_id: uxrObjectId,
    ) -> u16;
}

// ============================================================================
// Safe constructors for opaque types
// ============================================================================

impl uxrSession {
    /// Create a zeroed session blob. Must be initialized with `uxr_init_session`.
    pub fn zeroed() -> Self {
        Self {
            _opaque: [0u8; UXR_SESSION_SIZE],
        }
    }
}

impl uxrCustomTransport {
    /// Create a zeroed transport blob. Must be initialized via
    /// `uxr_set_custom_transport_callbacks` + `uxr_init_custom_transport`.
    pub fn zeroed() -> Self {
        Self {
            _opaque: [0u8; UXR_CUSTOM_TRANSPORT_SIZE],
        }
    }
}
