//! Support context for nros C API.
//!
//! The support context manages the underlying zenoh session and provides
//! shared resources for nodes, publishers, and subscribers.

use core::ffi::{c_char, c_int};
use core::ptr;

use crate::constants::MAX_LOCATOR_LEN;
use crate::error::*;

/// Support context state
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nano_ros_support_state_t {
    /// Not initialized
    NANO_ROS_SUPPORT_STATE_UNINITIALIZED = 0,
    /// Initialized and ready
    NANO_ROS_SUPPORT_STATE_INITIALIZED = 1,
    /// Shutdown
    NANO_ROS_SUPPORT_STATE_SHUTDOWN = 2,
}

/// Support context structure.
///
/// This is the main context for nros, similar to rclc_support_t.
/// It manages the zenoh session and provides shared resources.
#[repr(C)]
pub struct nano_ros_support_t {
    /// Current state
    pub state: nano_ros_support_state_t,
    /// Domain ID (ROS_DOMAIN_ID)
    pub domain_id: u8,
    /// Locator string storage
    locator: [u8; MAX_LOCATOR_LEN],
    /// Locator string length
    locator_len: usize,
    /// Opaque pointer to internal Rust context
    /// This will hold a pointer to the zenoh session
    _internal: *mut core::ffi::c_void,
}

impl Default for nano_ros_support_t {
    fn default() -> Self {
        Self {
            state: nano_ros_support_state_t::NANO_ROS_SUPPORT_STATE_UNINITIALIZED,
            domain_id: 0,
            locator: [0u8; MAX_LOCATOR_LEN],
            locator_len: 0,
            _internal: ptr::null_mut(),
        }
    }
}

/// Get a zero-initialized support context.
///
/// # Safety
/// Returns a stack-allocated struct that must be initialized before use.
#[unsafe(no_mangle)]
pub extern "C" fn nano_ros_support_get_zero_initialized() -> nano_ros_support_t {
    nano_ros_support_t::default()
}

/// Initialize the support context.
///
/// This function initializes the zenoh session and prepares the context
/// for creating nodes, publishers, and subscribers.
///
/// # Parameters
/// * `support` - Pointer to a zero-initialized support context
/// * `locator` - Zenoh locator string (e.g., "tcp/127.0.0.1:7447"), or NULL for default
/// * `domain_id` - ROS domain ID (0-232)
///
/// # Returns
/// * `NANO_ROS_RET_OK` on success
/// * `NANO_ROS_RET_INVALID_ARGUMENT` if support is NULL
/// * `NANO_ROS_RET_ERROR` on initialization failure
///
/// # Safety
/// * `support` must be a valid pointer to a zero-initialized nano_ros_support_t
/// * `locator` must be a valid null-terminated string or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_support_init(
    support: *mut nano_ros_support_t,
    locator: *const c_char,
    domain_id: u8,
) -> nano_ros_ret_t {
    if support.is_null() {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }

    let support = &mut *support;

    // Check if already initialized
    if support.state != nano_ros_support_state_t::NANO_ROS_SUPPORT_STATE_UNINITIALIZED {
        return NANO_ROS_RET_BAD_SEQUENCE;
    }

    // Store domain ID
    support.domain_id = domain_id;

    // Copy locator string if provided
    if !locator.is_null() {
        let mut len = 0usize;
        let locator_ptr = locator as *const u8;
        while len < MAX_LOCATOR_LEN - 1 {
            let c = *locator_ptr.add(len);
            if c == 0 {
                break;
            }
            support.locator[len] = c;
            len += 1;
        }
        support.locator[len] = 0;
        support.locator_len = len;
    } else {
        // Default locator
        let default_locator = b"tcp/127.0.0.1:7447\0";
        let len = default_locator.len() - 1;
        support.locator[..len].copy_from_slice(&default_locator[..len]);
        support.locator[len] = 0;
        support.locator_len = len;
    }

    // Initialize the zenoh session
    // For now, we'll use the shim directly
    #[cfg(feature = "alloc")]
    {
        use nano_ros_transport::{SessionMode, ShimSession, TransportConfig};

        let locator_str = core::str::from_utf8_unchecked(&support.locator[..support.locator_len]);

        let config = TransportConfig {
            locator: Some(locator_str),
            mode: SessionMode::Client,
            properties: &[],
        };

        match ShimSession::new(&config) {
            Ok(session) => {
                // Store the session pointer
                let session_box = alloc::boxed::Box::new(session);
                support._internal = alloc::boxed::Box::into_raw(session_box) as *mut _;
                support.state = nano_ros_support_state_t::NANO_ROS_SUPPORT_STATE_INITIALIZED;
                NANO_ROS_RET_OK
            }
            Err(_) => NANO_ROS_RET_ERROR,
        }
    }

    #[cfg(not(feature = "alloc"))]
    {
        // For no_std, we need to use the shim transport
        // This will be implemented when shim support is added
        NANO_ROS_RET_ERROR
    }
}

/// Finalize the support context.
///
/// This function closes the zenoh session and releases all resources.
///
/// # Parameters
/// * `support` - Pointer to an initialized support context
///
/// # Returns
/// * `NANO_ROS_RET_OK` on success
/// * `NANO_ROS_RET_INVALID_ARGUMENT` if support is NULL
/// * `NANO_ROS_RET_NOT_INIT` if not initialized
///
/// # Safety
/// * `support` must be a valid pointer to an initialized nano_ros_support_t
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_support_fini(support: *mut nano_ros_support_t) -> nano_ros_ret_t {
    if support.is_null() {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }

    let support = &mut *support;

    if support.state != nano_ros_support_state_t::NANO_ROS_SUPPORT_STATE_INITIALIZED {
        return NANO_ROS_RET_NOT_INIT;
    }

    // Clean up the session
    #[cfg(feature = "alloc")]
    {
        if !support._internal.is_null() {
            use nano_ros_transport::ShimSession;
            let _session = alloc::boxed::Box::from_raw(support._internal as *mut ShimSession);
            // Session is dropped here
        }
    }

    support._internal = ptr::null_mut();
    support.state = nano_ros_support_state_t::NANO_ROS_SUPPORT_STATE_SHUTDOWN;

    NANO_ROS_RET_OK
}

/// Check if support context is valid (initialized).
///
/// # Parameters
/// * `support` - Pointer to a support context
///
/// # Returns
/// * Non-zero if valid, 0 if invalid or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_support_is_valid(support: *const nano_ros_support_t) -> c_int {
    if support.is_null() {
        return 0;
    }

    let support = &*support;
    if support.state == nano_ros_support_state_t::NANO_ROS_SUPPORT_STATE_INITIALIZED {
        1
    } else {
        0
    }
}

#[cfg(kani)]
mod verification {
    use super::*;
    use crate::error::*;

    #[kani::proof]
    #[kani::unwind(5)]
    fn support_init_null_ptr() {
        // NULL support pointer → INVALID_ARGUMENT
        let ret = unsafe { nano_ros_support_init(core::ptr::null_mut(), core::ptr::null(), 0) };
        assert_eq!(ret, NANO_ROS_RET_INVALID_ARGUMENT);
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn support_zero_initialized_state() {
        let support = nano_ros_support_get_zero_initialized();
        assert_eq!(
            support.state,
            nano_ros_support_state_t::NANO_ROS_SUPPORT_STATE_UNINITIALIZED
        );
        assert_eq!(support.domain_id, 0);
        assert!(support._internal.is_null());
    }
}

impl nano_ros_support_t {
    /// Get the internal session pointer (for internal use)
    #[cfg(feature = "alloc")]
    pub(crate) unsafe fn get_session(&self) -> Option<&nano_ros_transport::ShimSession> {
        if self._internal.is_null() {
            None
        } else {
            Some(&*(self._internal as *const nano_ros_transport::ShimSession))
        }
    }

    /// Get the internal session pointer mutably (for internal use)
    #[cfg(feature = "alloc")]
    pub(crate) unsafe fn get_session_mut(
        &mut self,
    ) -> Option<&mut nano_ros_transport::ShimSession> {
        if self._internal.is_null() {
            None
        } else {
            Some(&mut *(self._internal as *mut nano_ros_transport::ShimSession))
        }
    }

    /// Get the locator string
    pub(crate) fn get_locator(&self) -> &[u8] {
        &self.locator[..self.locator_len]
    }
}
