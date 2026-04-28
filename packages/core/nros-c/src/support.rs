//! Support context for nros C API.
//!
//! The support context manages the underlying middleware session and provides
//! shared resources for nodes, publishers, and subscribers.

use core::ffi::c_char;

use crate::constants::{MAX_LOCATOR_LEN, SESSION_OPAQUE_U64S};
use crate::error::*;

/// Support context state
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nros_support_state_t {
    /// Not initialized
    NROS_SUPPORT_STATE_UNINITIALIZED = 0,
    /// Initialized and ready
    NROS_SUPPORT_STATE_INITIALIZED = 1,
    /// Shutdown
    NROS_SUPPORT_STATE_SHUTDOWN = 2,
}

/// Support context structure.
///
/// This is the main context for nros, similar to rclc_support_t.
/// It manages the middleware session and provides shared resources.
#[repr(C)]
pub struct nros_support_t {
    /// Current state
    pub state: nros_support_state_t,
    /// Domain ID (ROS_DOMAIN_ID)
    pub domain_id: u8,
    /// Locator string storage
    pub locator: [u8; MAX_LOCATOR_LEN],
    /// Locator string length
    pub locator_len: usize,
    /// Inline opaque storage for the RMW session.
    /// Avoids heap allocation — managed by nros_support_init/fini.
    pub _opaque: [u64; SESSION_OPAQUE_U64S],
}

// SESSION_OPAQUE_U64S is computed from size_of::<RmwSession>() in opaque_sizes.rs —
// always large enough by construction.

impl Default for nros_support_t {
    fn default() -> Self {
        Self {
            state: nros_support_state_t::NROS_SUPPORT_STATE_UNINITIALIZED,
            domain_id: 0,
            locator: [0u8; MAX_LOCATOR_LEN],
            locator_len: 0,
            _opaque: [0u64; SESSION_OPAQUE_U64S],
        }
    }
}

/// Get a zero-initialized support context.
///
/// # Safety
/// Returns a stack-allocated struct that must be initialized before use.
#[unsafe(no_mangle)]
pub extern "C" fn nros_support_get_zero_initialized() -> nros_support_t {
    nros_support_t::default()
}

/// Initialize the support context.
///
/// This function initializes the middleware session and prepares the context
/// for creating nodes, publishers, and subscribers.
///
/// # Parameters
/// * `support` - Pointer to a zero-initialized support context
/// * `locator` - Middleware locator string (e.g., "tcp/127.0.0.1:7447"), or NULL for default
/// * `domain_id` - ROS domain ID (0-232)
///
/// # Returns
/// * `NROS_RET_OK` on success
/// * `NROS_RET_INVALID_ARGUMENT` if support is NULL
/// * `NROS_RET_ERROR` on initialization failure
///
/// # Safety
/// * `support` must be a valid pointer to a zero-initialized nros_support_t
/// * `locator` must be a valid null-terminated string or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_support_init(
    support: *mut nros_support_t,
    locator: *const c_char,
    domain_id: u8,
) -> nros_ret_t {
    unsafe { nros_support_init_named(support, locator, domain_id, core::ptr::null()) }
}

/// Initialize the support context with a session name.
///
/// Like `nros_support_init`, but allows specifying a session name for
/// XRCE-DDS. Different XRCE clients on the same agent MUST use different
/// session names; otherwise the agent treats them as the same client and
/// won't relay data between them.
///
/// # Parameters
/// * `support` - Pointer to a zero-initialized support context
/// * `locator` - Middleware locator string, or NULL for default
/// * `domain_id` - ROS domain ID (0-232)
/// * `session_name` - Session name for XRCE key derivation, or NULL for default
///
/// # Safety
/// * `support` must be a valid pointer to a zero-initialized nros_support_t
/// * `locator` and `session_name` must be valid null-terminated strings or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_support_init_named(
    support: *mut nros_support_t,
    locator: *const c_char,
    domain_id: u8,
    session_name: *const c_char,
) -> nros_ret_t {
    if support.is_null() {
        return NROS_RET_INVALID_ARGUMENT;
    }

    let support = &mut *support;

    // Check if already initialized
    if support.state != nros_support_state_t::NROS_SUPPORT_STATE_UNINITIALIZED {
        return NROS_RET_BAD_SEQUENCE;
    }

    // Store domain ID
    support.domain_id = domain_id;

    // Copy locator string if provided (helper handles the null branch
    // gracefully — but the legacy `else` branch installs a backend-default
    // locator instead of leaving the field empty, so the explicit
    // null-check stays).
    if !locator.is_null() {
        support.locator_len = crate::util::copy_cstr_into(locator, &mut support.locator);
    } else {
        // Backend-dependent default locator
        #[cfg(feature = "rmw-zenoh")]
        let default_locator = b"tcp/127.0.0.1:7447\0";
        #[cfg(all(feature = "rmw-xrce", not(feature = "rmw-zenoh")))]
        let default_locator = b"127.0.0.1:2019\0";
        // DDS doesn't use a locator string — discovery is via SPDP on the
        // configured domain_id. Empty default is fine.
        #[cfg(all(
            feature = "rmw-dds",
            not(feature = "rmw-zenoh"),
            not(feature = "rmw-xrce")
        ))]
        let default_locator = b"\0";
        #[cfg(not(any(feature = "rmw-zenoh", feature = "rmw-xrce", feature = "rmw-dds")))]
        let default_locator = b"\0";

        let len = default_locator.len() - 1;
        support.locator[..len].copy_from_slice(&default_locator[..len]);
        support.locator[len] = 0;
        support.locator_len = len;
    }

    // Initialize the middleware session
    #[cfg(any(feature = "rmw-zenoh", feature = "rmw-xrce", feature = "rmw-dds"))]
    {
        use nros_node::SessionMode;

        let locator_str = core::str::from_utf8_unchecked(&support.locator[..support.locator_len]);

        // Derive session name. XRCE uses this to generate a unique session key —
        // two processes with the same name on the same agent will collide.
        let name: nros_core::heapless::String<32> = if !session_name.is_null() {
            // Use caller-provided name
            let name_cstr = core::ffi::CStr::from_ptr(session_name);
            nros_core::heapless::String::try_from(name_cstr.to_str().unwrap_or("nros"))
                .unwrap_or_else(|_| nros_core::heapless::String::try_from("nros").unwrap())
        } else {
            // Auto-generate: use PID on std, fallback on no_std
            #[cfg(feature = "std")]
            {
                let mut buf = nros_core::heapless::String::<32>::new();
                let _ = core::fmt::Write::write_fmt(
                    &mut buf,
                    format_args!("nros_{}", std::process::id()),
                );
                buf
            }
            #[cfg(not(feature = "std"))]
            {
                nros_core::heapless::String::try_from("nros").unwrap()
            }
        };

        match nros::internals::open_session(
            locator_str,
            SessionMode::Client,
            support.domain_id as u32,
            &name,
        ) {
            Ok(session) => {
                // Write session directly into inline opaque storage
                core::ptr::write(
                    support._opaque.as_mut_ptr() as *mut nros::internals::RmwSession,
                    session,
                );
                support.state = nros_support_state_t::NROS_SUPPORT_STATE_INITIALIZED;
                NROS_RET_OK
            }
            Err(_) => NROS_RET_ERROR,
        }
    }

    #[cfg(not(any(feature = "rmw-zenoh", feature = "rmw-xrce", feature = "rmw-dds")))]
    {
        NROS_RET_ERROR
    }
}

/// Finalize the support context.
///
/// This function closes the middleware session and releases all resources.
///
/// # Parameters
/// * `support` - Pointer to an initialized support context
///
/// # Returns
/// * `NROS_RET_OK` on success
/// * `NROS_RET_INVALID_ARGUMENT` if support is NULL
/// * `NROS_RET_NOT_INIT` if not initialized
///
/// # Safety
/// * `support` must be a valid pointer to an initialized nros_support_t
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_support_fini(support: *mut nros_support_t) -> nros_ret_t {
    if support.is_null() {
        return NROS_RET_INVALID_ARGUMENT;
    }

    let support = &mut *support;

    if support.state != nros_support_state_t::NROS_SUPPORT_STATE_INITIALIZED {
        return NROS_RET_NOT_INIT;
    }

    // Drop the inline RMW session
    #[cfg(any(feature = "rmw-zenoh", feature = "rmw-xrce", feature = "rmw-dds"))]
    {
        core::ptr::drop_in_place(support._opaque.as_mut_ptr() as *mut nros::internals::RmwSession);
    }

    support._opaque = [0u64; SESSION_OPAQUE_U64S];
    support.state = nros_support_state_t::NROS_SUPPORT_STATE_SHUTDOWN;

    NROS_RET_OK
}

/// Check if support context is valid (initialized).
///
/// # Parameters
/// * `support` - Pointer to a support context
///
/// # Returns
/// * `true` if valid, `false` if invalid or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_support_is_valid(support: *const nros_support_t) -> bool {
    if support.is_null() {
        return false;
    }

    let support = &*support;
    support.state == nros_support_state_t::NROS_SUPPORT_STATE_INITIALIZED
}

#[cfg(kani)]
mod verification {
    use super::*;
    use crate::error::*;

    #[kani::proof]
    #[kani::unwind(5)]
    fn support_init_null_ptr() {
        // NULL support pointer → INVALID_ARGUMENT
        let ret = unsafe { nros_support_init(core::ptr::null_mut(), core::ptr::null(), 0) };
        assert_eq!(ret, NROS_RET_INVALID_ARGUMENT);
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn support_zero_initialized_state() {
        let support = nros_support_get_zero_initialized();
        assert_eq!(
            support.state,
            nros_support_state_t::NROS_SUPPORT_STATE_UNINITIALIZED
        );
        assert_eq!(support.domain_id, 0);
        assert!(support._opaque.iter().all(|&v| v == 0));
    }
}

impl nros_support_t {
    /// Get the raw session pointer (for executor initialization).
    pub(crate) fn get_session_ptr(&self) -> *mut nros::internals::RmwSession {
        self._opaque.as_ptr() as *mut nros::internals::RmwSession
    }

    /// Get the internal session reference (for internal use)
    pub(crate) unsafe fn get_session(&self) -> Option<&nros::internals::RmwSession> {
        if self.state != nros_support_state_t::NROS_SUPPORT_STATE_INITIALIZED {
            None
        } else {
            Some(&*(self._opaque.as_ptr() as *const nros::internals::RmwSession))
        }
    }

    /// Get the internal session reference mutably (for internal use)
    pub(crate) unsafe fn get_session_mut(&mut self) -> Option<&mut nros::internals::RmwSession> {
        if self.state != nros_support_state_t::NROS_SUPPORT_STATE_INITIALIZED {
            None
        } else {
            Some(&mut *(self._opaque.as_mut_ptr() as *mut nros::internals::RmwSession))
        }
    }

    /// Get the locator string
    pub(crate) fn get_locator(&self) -> &[u8] {
        &self.locator[..self.locator_len]
    }
}
