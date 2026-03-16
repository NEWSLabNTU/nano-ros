//! High-level Rust API for zenoh-pico
//!
//! This module provides a safe Rust wrapper around the zenoh-pico C shim,
//! enabling embedded applications to use zenoh for communication.

#[cfg(any(
    feature = "platform-posix",
    feature = "platform-zephyr",
    feature = "platform-bare-metal",
    feature = "platform-freertos",
    feature = "platform-nuttx",
    feature = "platform-threadx"
))]
use core::ffi::c_void;
#[cfg(any(
    feature = "platform-posix",
    feature = "platform-zephyr",
    feature = "platform-bare-metal",
    feature = "platform-freertos",
    feature = "platform-nuttx",
    feature = "platform-threadx"
))]
use core::marker::PhantomData;

// ============================================================================
// FFI Reentrancy Guard
// ============================================================================

/// Execute a closure with FFI reentrancy protection.
///
/// When the `ffi-sync` feature is enabled, wraps the closure in
/// `critical_section::with()` to prevent concurrent access to zpico global
/// state from mixed-priority tasks or ISRs.
///
/// When the feature is disabled, this is a zero-cost passthrough.
#[allow(dead_code)] // used only when a platform feature is enabled
#[inline(always)]
pub(crate) fn ffi_guard<R>(f: impl FnOnce() -> R) -> R {
    #[cfg(feature = "ffi-sync")]
    {
        return critical_section::with(|_cs| f());
    }
    #[cfg(not(feature = "ffi-sync"))]
    f()
}

// Re-export FFI types and constants from sys crate
pub use zpico_sys::{
    ZPICO_ERR_CONFIG, ZPICO_ERR_FULL, ZPICO_ERR_GENERIC, ZPICO_ERR_INVALID, ZPICO_ERR_KEYEXPR,
    ZPICO_ERR_PUBLISH, ZPICO_ERR_SESSION, ZPICO_ERR_TASK, ZPICO_ERR_TIMEOUT, ZPICO_MAX_LIVELINESS,
    ZPICO_MAX_PENDING_GETS, ZPICO_MAX_PUBLISHERS, ZPICO_MAX_QUERYABLES, ZPICO_MAX_SUBSCRIBERS,
    ZPICO_OK, ZPICO_RMW_GID_SIZE, ZPICO_ZID_SIZE, ZpicoCallback, ZpicoCallbackWithAttachment,
    ZpicoNotifyCallback, ZpicoQueryCallback, ZpicoZeroCopyCallback, zpico_property_t,
};

// Import FFI functions from sys crate
#[cfg(any(
    feature = "platform-posix",
    feature = "platform-zephyr",
    feature = "platform-bare-metal",
    feature = "platform-freertos",
    feature = "platform-nuttx",
    feature = "platform-threadx"
))]
use zpico_sys::{
    zpico_close, zpico_declare_liveliness, zpico_declare_publisher, zpico_declare_queryable,
    zpico_declare_subscriber, zpico_declare_subscriber_direct_write,
    zpico_declare_subscriber_with_attachment, zpico_get, zpico_get_zid, zpico_init,
    zpico_init_with_config, zpico_is_open, zpico_open, zpico_poll, zpico_publish,
    zpico_publish_with_attachment, zpico_query_reply, zpico_spin_once, zpico_subscribe_zero_copy,
    zpico_undeclare_liveliness, zpico_undeclare_publisher, zpico_undeclare_queryable,
    zpico_undeclare_subscriber, zpico_uses_polling,
};

// ============================================================================
// Error Types
// ============================================================================

/// Error type for shim operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZpicoError {
    /// Generic error
    Generic,
    /// Configuration error
    Config,
    /// Session error
    Session,
    /// Task creation error
    Task,
    /// Invalid key expression
    KeyExpr,
    /// Resource limit reached
    Full,
    /// Invalid handle
    Invalid,
    /// Publish error
    Publish,
    /// Session not open
    NotOpen,
    /// Query timeout (no reply received)
    Timeout,
}

#[cfg(any(
    feature = "platform-posix",
    feature = "platform-zephyr",
    feature = "platform-bare-metal",
    feature = "platform-freertos",
    feature = "platform-nuttx",
    feature = "platform-threadx",
    test
))]
impl ZpicoError {
    fn from_code(code: i32) -> Self {
        match code {
            -1 => ZpicoError::Generic,
            -2 => ZpicoError::Config,
            -3 => ZpicoError::Session,
            -4 => ZpicoError::Task,
            -5 => ZpicoError::KeyExpr,
            -6 => ZpicoError::Full,
            -7 => ZpicoError::Invalid,
            -8 => ZpicoError::Publish,
            _ => ZpicoError::Generic,
        }
    }
}

impl core::fmt::Display for ZpicoError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ZpicoError::Generic => write!(f, "generic error"),
            ZpicoError::Config => write!(f, "configuration error"),
            ZpicoError::Session => write!(f, "session error"),
            ZpicoError::Task => write!(f, "task creation error"),
            ZpicoError::KeyExpr => write!(f, "invalid key expression"),
            ZpicoError::Full => write!(f, "resource limit reached"),
            ZpicoError::Invalid => write!(f, "invalid handle"),
            ZpicoError::Publish => write!(f, "publish error"),
            ZpicoError::NotOpen => write!(f, "session not open"),
            ZpicoError::Timeout => write!(f, "query timeout"),
        }
    }
}

/// Result type for shim operations
pub type Result<T> = core::result::Result<T, ZpicoError>;

// ============================================================================
// ZenohId
// ============================================================================

/// A 16-byte Zenoh session ID
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ZenohId {
    /// The raw 16-byte ID
    pub id: [u8; 16],
}

impl ZenohId {
    /// Create a new ZenohId from raw bytes
    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        Self { id: bytes }
    }

    // `to_hex_string()` removed — was dead code requiring `alloc`.
    // Use `to_hex_bytes()` instead (alloc-free, writes into caller-provided buffer).

    /// Format the ID into a fixed-size buffer (for no_std)
    ///
    /// Returns the number of bytes written (always 32).
    pub fn to_hex_bytes(&self, buf: &mut [u8; 32]) {
        const HEX_CHARS: &[u8; 16] = b"0123456789abcdef";
        // LSB-first order
        for i in 0..16 {
            let byte = self.id[15 - i];
            buf[i * 2] = HEX_CHARS[(byte >> 4) as usize];
            buf[i * 2 + 1] = HEX_CHARS[(byte & 0xf) as usize];
        }
    }
}

// ============================================================================
// LivelinessToken
// ============================================================================

/// A liveliness token for ROS 2 discovery
///
/// When a liveliness token is declared, subscribers on intersecting key expressions
/// will receive a PUT sample when connectivity is achieved, and a DELETE sample
/// if it's lost.
///
/// Liveliness tokens are automatically undeclared when dropped.
///
/// Note: The C shim manages tokens via static storage with integer handles,
/// so the token does not need a lifetime parameter.
#[cfg(any(
    feature = "platform-posix",
    feature = "platform-zephyr",
    feature = "platform-bare-metal",
    feature = "platform-freertos",
    feature = "platform-nuttx",
    feature = "platform-threadx"
))]
pub struct LivelinessToken {
    handle: i32,
}

#[cfg(any(
    feature = "platform-posix",
    feature = "platform-zephyr",
    feature = "platform-bare-metal",
    feature = "platform-freertos",
    feature = "platform-nuttx",
    feature = "platform-threadx"
))]
impl LivelinessToken {
    /// Get the liveliness handle
    pub fn handle(&self) -> i32 {
        self.handle
    }
}

#[cfg(any(
    feature = "platform-posix",
    feature = "platform-zephyr",
    feature = "platform-bare-metal",
    feature = "platform-freertos",
    feature = "platform-nuttx",
    feature = "platform-threadx"
))]
impl Drop for LivelinessToken {
    fn drop(&mut self) {
        ffi_guard(|| unsafe {
            zpico_undeclare_liveliness(self.handle);
        });
    }
}

// ============================================================================
// Queryable
// ============================================================================

/// A queryable for receiving service requests
///
/// Queryables receive queries and can send replies. This is used to implement
/// ROS 2 service servers.
///
/// Note: The C shim manages queryables via static storage with integer handles,
/// so the queryable does not need a lifetime parameter.
#[cfg(any(
    feature = "platform-posix",
    feature = "platform-zephyr",
    feature = "platform-bare-metal",
    feature = "platform-freertos",
    feature = "platform-nuttx",
    feature = "platform-threadx"
))]
pub struct Queryable {
    handle: i32,
}

#[cfg(any(
    feature = "platform-posix",
    feature = "platform-zephyr",
    feature = "platform-bare-metal",
    feature = "platform-freertos",
    feature = "platform-nuttx",
    feature = "platform-threadx"
))]
impl Queryable {
    /// Get the queryable handle
    pub fn handle(&self) -> i32 {
        self.handle
    }
}

#[cfg(any(
    feature = "platform-posix",
    feature = "platform-zephyr",
    feature = "platform-bare-metal",
    feature = "platform-freertos",
    feature = "platform-nuttx",
    feature = "platform-threadx"
))]
impl Drop for Queryable {
    fn drop(&mut self) {
        ffi_guard(|| unsafe {
            zpico_undeclare_queryable(self.handle);
        });
    }
}

// ============================================================================
// Context
// ============================================================================

/// Context for managing zenoh-pico shim session
///
/// The context manages the zenoh session lifecycle and provides methods
/// for creating publishers and subscribers.
///
/// # Note
///
/// Only one `Context` can exist at a time due to the global state
/// in the C shim.
#[cfg(any(
    feature = "platform-posix",
    feature = "platform-zephyr",
    feature = "platform-bare-metal",
    feature = "platform-freertos",
    feature = "platform-nuttx",
    feature = "platform-threadx"
))]
pub struct Context {
    _private: PhantomData<*const ()>,
}

#[cfg(any(
    feature = "platform-posix",
    feature = "platform-zephyr",
    feature = "platform-bare-metal",
    feature = "platform-freertos",
    feature = "platform-nuttx",
    feature = "platform-threadx"
))]
impl Context {
    /// Create a new shim context with the given locator
    ///
    /// The locator should be a null-terminated string like `b"tcp/127.0.0.1:7447\0"`.
    ///
    /// # Errors
    ///
    /// Returns an error if initialization or session opening fails.
    pub fn new(locator: &[u8]) -> Result<Self> {
        ffi_guard(|| {
            // Safety: locator is a valid byte slice, cast to c_char for C string
            let ret = unsafe { zpico_init(locator.as_ptr().cast()) };
            if ret < 0 {
                return Err(ZpicoError::from_code(ret));
            }

            let ret = unsafe { zpico_open() };
            if ret < 0 {
                return Err(ZpicoError::from_code(ret));
            }

            Ok(Context {
                _private: PhantomData,
            })
        })
    }

    /// Create a new shim context with mode, locator, and properties
    ///
    /// Byte slices for locator and mode must be null-terminated C strings.
    ///
    /// # Arguments
    ///
    /// * `locator` - Null-terminated locator (e.g., `b"tcp/127.0.0.1:7447\0"`), or `None` for peer mode
    /// * `mode` - Null-terminated mode string (`b"client\0"` or `b"peer\0"`)
    /// * `properties` - Array of C-compatible key-value properties
    ///
    /// # Errors
    ///
    /// Returns an error if initialization or session opening fails.
    pub fn with_config(
        locator: Option<&[u8]>,
        mode: &[u8],
        properties: &[zpico_sys::zpico_property_t],
    ) -> Result<Self> {
        let locator_ptr = match locator {
            Some(loc) => loc.as_ptr().cast(),
            None => core::ptr::null(),
        };
        let props_ptr = if properties.is_empty() {
            core::ptr::null()
        } else {
            properties.as_ptr()
        };
        ffi_guard(|| {
            let ret = unsafe {
                zpico_init_with_config(
                    locator_ptr,
                    mode.as_ptr().cast(),
                    props_ptr,
                    properties.len(),
                )
            };
            if ret < 0 {
                return Err(ZpicoError::from_code(ret));
            }

            let ret = unsafe { zpico_open() };
            if ret < 0 {
                return Err(ZpicoError::from_code(ret));
            }

            Ok(Context {
                _private: PhantomData,
            })
        })
    }

    /// Check if the session is open
    pub fn is_open(&self) -> bool {
        ffi_guard(|| unsafe { zpico_is_open() != 0 })
    }

    /// Check if this backend uses polling
    ///
    /// If true, you must call `poll()` or `spin_once()` regularly to
    /// process network data and dispatch callbacks.
    pub fn uses_polling(&self) -> bool {
        ffi_guard(|| unsafe { zpico_uses_polling() })
    }

    /// Declare a publisher for the given key expression
    ///
    /// The key expression should be a null-terminated string like `b"demo/topic\0"`.
    ///
    /// # Errors
    ///
    /// Returns an error if the session is not open, the key expression is invalid,
    /// or the maximum number of publishers has been reached.
    pub fn declare_publisher(&self, keyexpr: &[u8]) -> Result<Publisher<'_>> {
        let handle = ffi_guard(|| unsafe { zpico_declare_publisher(keyexpr.as_ptr().cast()) });
        if handle < 0 {
            return Err(ZpicoError::from_code(handle));
        }

        Ok(Publisher {
            handle,
            _ctx: PhantomData,
        })
    }

    /// Declare a subscriber for the given key expression
    ///
    /// The key expression should be a null-terminated string like `b"demo/topic\0"`.
    /// The callback will be invoked when samples arrive.
    ///
    /// # Safety
    ///
    /// The callback and context must remain valid for the lifetime of the subscriber.
    /// The context pointer must be valid for the callback to dereference.
    ///
    /// # Errors
    ///
    /// Returns an error if the session is not open, the key expression is invalid,
    /// or the maximum number of subscribers has been reached.
    pub unsafe fn declare_subscriber_raw<'a>(
        &'a self,
        keyexpr: &[u8],
        callback: ZpicoCallback,
        ctx: *mut c_void,
    ) -> Result<Subscriber<'a>> {
        let handle = ffi_guard(|| unsafe {
            zpico_declare_subscriber(keyexpr.as_ptr().cast(), callback, ctx)
        });
        if handle < 0 {
            return Err(ZpicoError::from_code(handle));
        }

        Ok(Subscriber {
            handle,
            _ctx: PhantomData,
        })
    }

    /// Declare a subscriber with attachment support for RMW compatibility
    ///
    /// The key expression should be a null-terminated string like `b"demo/topic\0"`.
    /// The callback will be invoked when samples arrive, with attachment data if present.
    ///
    /// # Safety
    ///
    /// The callback and context must remain valid for the lifetime of the subscriber.
    /// The context pointer must be valid for the callback to dereference.
    ///
    /// # Errors
    ///
    /// Returns an error if the session is not open, the key expression is invalid,
    /// or the maximum number of subscribers has been reached.
    pub unsafe fn declare_subscriber_with_attachment_raw<'a>(
        &'a self,
        keyexpr: &[u8],
        callback: ZpicoCallbackWithAttachment,
        ctx: *mut c_void,
    ) -> Result<Subscriber<'a>> {
        let handle = ffi_guard(|| unsafe {
            zpico_declare_subscriber_with_attachment(keyexpr.as_ptr().cast(), callback, ctx)
        });
        if handle < 0 {
            return Err(ZpicoError::from_code(handle));
        }

        Ok(Subscriber {
            handle,
            _ctx: PhantomData,
        })
    }

    /// Declare a subscriber with direct-write to a Rust buffer.
    ///
    /// The C shim reads the payload directly into `buf_ptr` using
    /// `z_bytes_reader_read()`, avoiding a malloc. The notify callback
    /// is called after the write, providing only the length and attachment.
    ///
    /// # Safety
    ///
    /// `buf_ptr` must point to valid memory for `buf_capacity` bytes that
    /// outlives the subscriber. `locked_ptr` must point to a valid `AtomicBool`.
    pub unsafe fn declare_subscriber_direct_write_raw<'a>(
        &'a self,
        keyexpr: &[u8],
        buf_ptr: *mut u8,
        buf_capacity: usize,
        locked_ptr: *const bool,
        callback: ZpicoNotifyCallback,
        ctx: *mut c_void,
    ) -> Result<Subscriber<'a>> {
        let handle = ffi_guard(|| unsafe {
            zpico_declare_subscriber_direct_write(
                keyexpr.as_ptr().cast(),
                buf_ptr,
                buf_capacity,
                locked_ptr,
                callback,
                ctx,
            )
        });
        if handle < 0 {
            return Err(ZpicoError::from_code(handle));
        }

        Ok(Subscriber {
            handle,
            _ctx: PhantomData,
        })
    }

    /// Declare a zero-copy subscriber for the given key expression.
    ///
    /// The callback receives a borrowed pointer directly into zenoh-pico's
    /// internal receive buffer. The pointer is only valid during the callback.
    ///
    /// # Safety
    ///
    /// The callback and context must remain valid for the lifetime of the subscriber.
    /// The data pointer passed to the callback is only valid during the callback invocation.
    pub unsafe fn subscribe_zero_copy_raw<'a>(
        &'a self,
        keyexpr: &[u8],
        callback: ZpicoZeroCopyCallback,
        ctx: *mut c_void,
    ) -> Result<Subscriber<'a>> {
        let handle = ffi_guard(|| unsafe {
            zpico_subscribe_zero_copy(keyexpr.as_ptr().cast(), callback, ctx)
        });
        if handle < 0 {
            return Err(ZpicoError::from_code(handle));
        }

        Ok(Subscriber {
            handle,
            _ctx: PhantomData,
        })
    }

    /// Poll for incoming data and process callbacks
    ///
    /// For threaded backends (POSIX, Zephyr), this is a no-op as background
    /// tasks handle polling automatically.
    ///
    /// For polling backends (smoltcp), this must be called regularly.
    ///
    /// # Arguments
    ///
    /// * `timeout_ms` - Maximum time to wait for data (0 = non-blocking)
    ///
    /// # Returns
    ///
    /// Number of events processed, or error
    pub fn poll(&self, timeout_ms: u32) -> Result<i32> {
        // When FFI guard is enabled, decompose blocking poll into a loop
        // of non-blocking guarded calls to keep critical sections short.
        #[cfg(feature = "ffi-sync")]
        {
            let ret = ffi_guard(|| unsafe { zpico_poll(0) });
            if ret < 0 {
                return Err(ZpicoError::from_code(ret));
            }
            if ret > 0 || timeout_ms == 0 {
                return Ok(ret);
            }
            // Loop with guarded non-blocking polls until timeout
            let mut clock = [0u8; 16];
            unsafe { zpico_sys::zpico_clock_start(clock.as_mut_ptr()) };
            loop {
                let elapsed =
                    unsafe { zpico_sys::zpico_clock_elapsed_ms_since(clock.as_mut_ptr()) };
                if elapsed >= timeout_ms as core::ffi::c_ulong {
                    return Ok(0);
                }
                let ret = ffi_guard(|| unsafe { zpico_poll(0) });
                if ret < 0 {
                    return Err(ZpicoError::from_code(ret));
                }
                if ret > 0 {
                    return Ok(ret);
                }
            }
        }
        #[cfg(not(feature = "ffi-sync"))]
        {
            let ret = unsafe { zpico_poll(timeout_ms) };
            if ret < 0 {
                return Err(ZpicoError::from_code(ret));
            }
            Ok(ret)
        }
    }

    /// Combined poll and keepalive operation
    ///
    /// This is equivalent to calling `poll()` and performing any necessary
    /// keepalive operations.
    ///
    /// # Arguments
    ///
    /// * `timeout_ms` - Maximum time to wait (0 = non-blocking)
    ///
    /// # Returns
    ///
    /// Number of events processed, or error
    pub fn spin_once(&self, timeout_ms: u32) -> Result<i32> {
        // When FFI guard is enabled, decompose blocking spin_once into a
        // loop of non-blocking guarded calls to keep critical sections short.
        #[cfg(feature = "ffi-sync")]
        {
            let ret = ffi_guard(|| unsafe { zpico_spin_once(0) });
            if ret < 0 {
                return Err(ZpicoError::from_code(ret));
            }
            if ret > 0 || timeout_ms == 0 {
                return Ok(ret);
            }
            // Loop with guarded non-blocking spin_once calls until timeout
            let mut clock = [0u8; 16];
            unsafe { zpico_sys::zpico_clock_start(clock.as_mut_ptr()) };
            loop {
                let elapsed =
                    unsafe { zpico_sys::zpico_clock_elapsed_ms_since(clock.as_mut_ptr()) };
                if elapsed >= timeout_ms as core::ffi::c_ulong {
                    return Ok(0);
                }
                let ret = ffi_guard(|| unsafe { zpico_spin_once(0) });
                if ret < 0 {
                    return Err(ZpicoError::from_code(ret));
                }
                if ret > 0 {
                    return Ok(ret);
                }
            }
        }
        #[cfg(not(feature = "ffi-sync"))]
        {
            let ret = unsafe { zpico_spin_once(timeout_ms) };
            if ret < 0 {
                return Err(ZpicoError::from_code(ret));
            }
            Ok(ret)
        }
    }

    /// Get the session's Zenoh ID
    ///
    /// The Zenoh ID uniquely identifies this session in the Zenoh network.
    /// It is used in liveliness token key expressions for ROS 2 discovery.
    pub fn zid(&self) -> Result<ZenohId> {
        let mut id = [0u8; 16];
        let ret = ffi_guard(|| unsafe { zpico_get_zid(id.as_mut_ptr()) });
        if ret < 0 {
            return Err(ZpicoError::from_code(ret));
        }
        Ok(ZenohId::from_bytes(id))
    }

    /// Declare a liveliness token for ROS 2 discovery
    ///
    /// The key expression should be a null-terminated string.
    ///
    /// # Errors
    ///
    /// Returns an error if the session is not open, the key expression is invalid,
    /// or the maximum number of liveliness tokens has been reached.
    pub fn declare_liveliness(&self, keyexpr: &[u8]) -> Result<LivelinessToken> {
        let handle = ffi_guard(|| unsafe { zpico_declare_liveliness(keyexpr.as_ptr().cast()) });
        if handle < 0 {
            return Err(ZpicoError::from_code(handle));
        }

        Ok(LivelinessToken { handle })
    }

    /// Declare a queryable for receiving service requests
    ///
    /// The key expression should be a null-terminated string.
    /// The callback will be invoked when queries arrive.
    ///
    /// # Safety
    ///
    /// The callback and context must remain valid for the lifetime of the queryable.
    /// The context pointer must be valid for the callback to dereference.
    ///
    /// # Errors
    ///
    /// Returns an error if the session is not open, the key expression is invalid,
    /// or the maximum number of queryables has been reached.
    pub unsafe fn declare_queryable_raw(
        &self,
        keyexpr: &[u8],
        callback: ZpicoQueryCallback,
        ctx: *mut c_void,
    ) -> Result<Queryable> {
        let handle = ffi_guard(|| unsafe {
            zpico_declare_queryable(keyexpr.as_ptr().cast(), callback, ctx)
        });
        if handle < 0 {
            return Err(ZpicoError::from_code(handle));
        }

        Ok(Queryable { handle })
    }

    /// Reply to a query (must be called within query callback)
    ///
    /// This sends a reply to the current query being processed.
    /// Must only be called from within a queryable callback.
    ///
    /// # Parameters
    ///
    /// * `queryable_handle` - Handle of the queryable that received the query
    /// * `keyexpr` - Reply key expression (null-terminated)
    /// * `data` - Reply payload
    /// * `attachment` - Optional attachment data
    ///
    /// # Errors
    ///
    /// Returns an error if the queryable handle is invalid, no stored query exists,
    /// or if the reply operation fails.
    pub fn query_reply(
        &self,
        queryable_handle: i32,
        keyexpr: &[u8],
        data: &[u8],
        attachment: Option<&[u8]>,
    ) -> Result<()> {
        let (att_ptr, att_len) = match attachment {
            Some(att) => (att.as_ptr(), att.len()),
            None => (core::ptr::null(), 0),
        };

        let ret = ffi_guard(|| unsafe {
            zpico_query_reply(
                queryable_handle,
                keyexpr.as_ptr().cast(),
                data.as_ptr(),
                data.len(),
                att_ptr,
                att_len,
            )
        });
        if ret < 0 {
            return Err(ZpicoError::from_code(ret));
        }
        Ok(())
    }

    /// Send a query and wait for reply (blocking, for service client)
    ///
    /// This sends a query to the given key expression and waits for a reply.
    /// Used to implement ROS 2 service clients.
    ///
    /// # Parameters
    ///
    /// * `keyexpr` - Key expression (null-terminated)
    /// * `payload` - Request payload (can be empty slice)
    /// * `reply_buf` - Buffer to receive reply data
    /// * `timeout_ms` - Timeout in milliseconds
    ///
    /// # Returns
    ///
    /// Number of bytes written to reply_buf on success.
    ///
    /// # Errors
    ///
    /// Returns an error if the query fails, times out, or if the reply is
    /// too large for the buffer.
    pub fn get(
        &self,
        keyexpr: &[u8],
        payload: &[u8],
        reply_buf: &mut [u8],
        timeout_ms: u32,
    ) -> Result<usize> {
        let (payload_ptr, payload_len) = if payload.is_empty() {
            (core::ptr::null(), 0)
        } else {
            (payload.as_ptr(), payload.len())
        };

        // When FFI guard is enabled, decompose into get_start + spin loop + get_check
        // to avoid holding the critical section for the full timeout duration.
        #[cfg(feature = "ffi-sync")]
        {
            let handle = ffi_guard(|| unsafe {
                zpico_sys::zpico_get_start(
                    keyexpr.as_ptr().cast(),
                    payload_ptr,
                    payload_len,
                    timeout_ms,
                )
            });
            if handle < 0 {
                return Err(ZpicoError::from_code(handle));
            }

            let mut clock = [0u8; 16];
            unsafe { zpico_sys::zpico_clock_start(clock.as_mut_ptr()) };
            loop {
                // Drive I/O with a non-blocking spin
                ffi_guard(|| unsafe { zpico_spin_once(0) });

                // Check for reply
                let ret = ffi_guard(|| unsafe {
                    zpico_sys::zpico_get_check(handle, reply_buf.as_mut_ptr(), reply_buf.len())
                });
                if ret > 0 {
                    return Ok(ret as usize);
                }
                if ret < 0 {
                    if ret == -9 {
                        return Err(ZpicoError::Timeout);
                    }
                    return Err(ZpicoError::from_code(ret));
                }

                // Check timeout
                let elapsed =
                    unsafe { zpico_sys::zpico_clock_elapsed_ms_since(clock.as_mut_ptr()) };
                if elapsed >= timeout_ms as core::ffi::c_ulong {
                    return Err(ZpicoError::Timeout);
                }
            }
        }
        #[cfg(not(feature = "ffi-sync"))]
        {
            let ret = unsafe {
                zpico_get(
                    keyexpr.as_ptr().cast(),
                    payload_ptr,
                    payload_len,
                    reply_buf.as_mut_ptr(),
                    reply_buf.len(),
                    timeout_ms,
                )
            };

            if ret < 0 {
                if ret == -9 {
                    return Err(ZpicoError::Timeout);
                }
                return Err(ZpicoError::from_code(ret));
            }

            Ok(ret as usize)
        }
    }

    /// Start a non-blocking query (for async service client).
    ///
    /// Returns a slot handle on success that can be polled with [`get_check()`](Self::get_check).
    pub fn get_start(&self, keyexpr: &[u8], payload: &[u8], timeout_ms: u32) -> Result<i32> {
        let (payload_ptr, payload_len) = if payload.is_empty() {
            (core::ptr::null(), 0)
        } else {
            (payload.as_ptr(), payload.len())
        };

        let ret = ffi_guard(|| unsafe {
            zpico_sys::zpico_get_start(
                keyexpr.as_ptr().cast(),
                payload_ptr,
                payload_len,
                timeout_ms,
            )
        });

        if ret < 0 {
            return Err(ZpicoError::from_code(ret));
        }

        Ok(ret)
    }

    /// Check for a reply to a pending non-blocking query.
    ///
    /// Returns `Ok(Some(len))` when a reply has arrived, `Ok(None)` if still
    /// pending, or `Err` on failure/timeout.
    pub fn get_check(&self, handle: i32, reply_buf: &mut [u8]) -> Result<Option<usize>> {
        let ret = ffi_guard(|| unsafe {
            zpico_sys::zpico_get_check(handle, reply_buf.as_mut_ptr(), reply_buf.len())
        });

        if ret > 0 {
            Ok(Some(ret as usize))
        } else if ret == 0 {
            Ok(None)
        } else {
            if ret == -9 {
                return Err(ZpicoError::Timeout);
            }
            Err(ZpicoError::from_code(ret))
        }
    }
}

#[cfg(any(
    feature = "platform-posix",
    feature = "platform-zephyr",
    feature = "platform-bare-metal",
    feature = "platform-freertos",
    feature = "platform-nuttx",
    feature = "platform-threadx"
))]
impl Drop for Context {
    fn drop(&mut self) {
        ffi_guard(|| unsafe {
            zpico_close();
        });
    }
}

// ============================================================================
// Publisher
// ============================================================================

/// Publisher handle for sending data
///
/// Created via `Context::declare_publisher()`.
#[cfg(any(
    feature = "platform-posix",
    feature = "platform-zephyr",
    feature = "platform-bare-metal",
    feature = "platform-freertos",
    feature = "platform-nuttx",
    feature = "platform-threadx"
))]
pub struct Publisher<'a> {
    handle: i32,
    _ctx: PhantomData<&'a Context>,
}

#[cfg(any(
    feature = "platform-posix",
    feature = "platform-zephyr",
    feature = "platform-bare-metal",
    feature = "platform-freertos",
    feature = "platform-nuttx",
    feature = "platform-threadx"
))]
impl<'a> Publisher<'a> {
    /// Publish data
    ///
    /// # Errors
    ///
    /// Returns an error if the publish operation fails.
    pub fn publish(&self, data: &[u8]) -> Result<()> {
        let ret = ffi_guard(|| unsafe { zpico_publish(self.handle, data.as_ptr(), data.len()) });
        if ret < 0 {
            return Err(ZpicoError::from_code(ret));
        }
        Ok(())
    }

    /// Publish data with an attachment
    ///
    /// This is used for RMW compatibility, where an attachment contains
    /// metadata like sequence number, timestamp, and GID.
    ///
    /// # Parameters
    ///
    /// * `data` - The message payload
    /// * `attachment` - Optional attachment data (for RMW compatibility)
    ///
    /// # Errors
    ///
    /// Returns an error if the publish operation fails.
    pub fn publish_with_attachment(&self, data: &[u8], attachment: Option<&[u8]>) -> Result<()> {
        let (att_ptr, att_len) = match attachment {
            Some(att) => (att.as_ptr(), att.len()),
            None => (core::ptr::null(), 0),
        };

        let ret = ffi_guard(|| unsafe {
            zpico_publish_with_attachment(self.handle, data.as_ptr(), data.len(), att_ptr, att_len)
        });
        if ret < 0 {
            return Err(ZpicoError::from_code(ret));
        }
        Ok(())
    }

    /// Get the publisher handle
    pub fn handle(&self) -> i32 {
        self.handle
    }
}

#[cfg(any(
    feature = "platform-posix",
    feature = "platform-zephyr",
    feature = "platform-bare-metal",
    feature = "platform-freertos",
    feature = "platform-nuttx",
    feature = "platform-threadx"
))]
impl<'a> Drop for Publisher<'a> {
    fn drop(&mut self) {
        ffi_guard(|| unsafe {
            zpico_undeclare_publisher(self.handle);
        });
    }
}

// ============================================================================
// Subscriber
// ============================================================================

/// Subscriber handle for receiving data
///
/// Created via `Context::declare_subscriber_raw()`.
#[cfg(any(
    feature = "platform-posix",
    feature = "platform-zephyr",
    feature = "platform-bare-metal",
    feature = "platform-freertos",
    feature = "platform-nuttx",
    feature = "platform-threadx"
))]
pub struct Subscriber<'a> {
    handle: i32,
    _ctx: PhantomData<&'a Context>,
}

#[cfg(any(
    feature = "platform-posix",
    feature = "platform-zephyr",
    feature = "platform-bare-metal",
    feature = "platform-freertos",
    feature = "platform-nuttx",
    feature = "platform-threadx"
))]
impl<'a> Subscriber<'a> {
    /// Get the subscriber handle
    pub fn handle(&self) -> i32 {
        self.handle
    }
}

#[cfg(any(
    feature = "platform-posix",
    feature = "platform-zephyr",
    feature = "platform-bare-metal",
    feature = "platform-freertos",
    feature = "platform-nuttx",
    feature = "platform-threadx"
))]
impl<'a> Drop for Subscriber<'a> {
    fn drop(&mut self) {
        ffi_guard(|| unsafe {
            zpico_undeclare_subscriber(self.handle);
        });
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    extern crate std;
    use std::format;

    #[test]
    fn test_error_from_code() {
        assert_eq!(ZpicoError::from_code(-1), ZpicoError::Generic);
        assert_eq!(ZpicoError::from_code(-2), ZpicoError::Config);
        assert_eq!(ZpicoError::from_code(-3), ZpicoError::Session);
        assert_eq!(ZpicoError::from_code(-4), ZpicoError::Task);
        assert_eq!(ZpicoError::from_code(-5), ZpicoError::KeyExpr);
        assert_eq!(ZpicoError::from_code(-6), ZpicoError::Full);
        assert_eq!(ZpicoError::from_code(-7), ZpicoError::Invalid);
        assert_eq!(ZpicoError::from_code(-8), ZpicoError::Publish);
        assert_eq!(ZpicoError::from_code(-99), ZpicoError::Generic);
    }

    #[test]
    fn test_error_display() {
        assert_eq!(format!("{}", ZpicoError::Generic), "generic error");
        assert_eq!(format!("{}", ZpicoError::Session), "session error");
    }
}
