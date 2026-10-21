//! Support context for nros C API.
//!
//! The support context manages the underlying middleware session and provides
//! shared resources for nodes, publishers, and subscribers.

use core::ffi::c_char;

use crate::{
    constants::{MAX_LOCATOR_LEN, SESSION_OPAQUE_U64S},
    error::*,
};

/// Phase 155.B — collapse `nros_rmw::TransportError` into the closest
/// matching `NROS_RET_*` so the C-side `rclc_support_init` error log
/// (`init -> -X`) tells the user which precondition the backend
/// rejected. Anything not directly classifiable falls back to
/// `RCL_RET_ERROR` (-1) — the legacy value, preserving the existing
/// "something went wrong, dig into the backend log" semantics for
/// callers that branch on `== RCL_RET_ERROR`.
#[cfg(feature = "rmw-cffi")]
pub(crate) fn transport_error_to_ret(err: nros_rmw::TransportError) -> rcl_ret_t {
    use nros_rmw::TransportError as E;
    match err {
        E::ConnectionFailed | E::Disconnected => RCLC_RET_NOT_FOUND,
        E::Timeout => RCL_RET_TIMEOUT,
        E::WouldBlock | E::NoData => RCLC_RET_TRY_AGAIN,
        E::InvalidConfig | E::InvalidArgument | E::TopicNameInvalid | E::NodeNameNonExistent => {
            RCL_RET_INVALID_ARGUMENT
        }
        E::BufferTooSmall | E::MessageTooLarge | E::TooLarge | E::BadAlloc => RCLC_RET_FULL,
        E::PublishFailed => RCLC_RET_PUBLISH_FAILED,
        E::ServiceRequestFailed | E::ServiceReplyFailed => RCLC_RET_SERVICE_FAILED,
        E::Unsupported | E::LoanNotSupported => RCLC_RET_NOT_ALLOWED,
        E::IncompatibleQos(_) | E::IncompatibleAbi => RCLC_RET_REJECTED,
        E::PublisherCreationFailed
        | E::SubscriberCreationFailed
        | E::ServiceServerCreationFailed
        | E::ServiceClientCreationFailed
        | E::SerializationError
        | E::DeserializationError
        | E::TaskStartFailed
        | E::PollFailed
        | E::KeepaliveFailed
        | E::JoinFailed => RCL_RET_ERROR,
        // Backend / BackendDynamic carry a backend-defined string that
        // can't ride through a `rcl_ret_t`. Keep them at the generic
        // catch-all so callers branching on `== RCL_RET_ERROR` still
        // catch backend faults; the string is lost (TODO: surface via
        // a side-channel debug-log macro for diagnostic builds).
        _ => RCL_RET_ERROR,
    }
}

/// Issue #227 — pass this as `domain_id` to `rclc_support_init[_named]` to
/// request an EXPLICIT domain 0. Plain `0` is the UNSET sentinel (defers to
/// `ROS_DOMAIN_ID` env on hosted, then the baked/default rungs — the #206
/// model-A ladder). Valid domains cap at 232, so 255 is unambiguous. Hosted
/// env still overrides it, like every explicit argument under model A.
pub const NROS_DOMAIN_ID_EXPLICIT_ZERO: u8 = 255;

/// Support context state
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum rclc_support_state_t {
    /// Not initialized
    RCLC_SUPPORT_STATE_UNINITIALIZED = 0,
    /// Initialized and ready
    RCLC_SUPPORT_STATE_INITIALIZED = 1,
    /// Shutdown
    RCLC_SUPPORT_STATE_SHUTDOWN = 2,
}

/// phase-492 W1.b — rcl's init context, as rclc programs reach it:
/// `&support.context`. Here it is the handle by which an executor finds the
/// support that owns the session; it carries nothing else. Upstream's members
/// (global arguments, instance id) are not modelled, so it is opaque to C.
#[repr(C)]
pub struct rcl_context_t {
    /// The support this context belongs to; NULL until `rclc_support_init`.
    pub _support: *const rclc_support_t,
}

/// The support a context names, if it is a live one: the context must be the
/// `context` member of the support it points back at, and that support
/// initialised.
///
/// # Safety
/// `context` must be NULL or point to a readable `rcl_context_t`.
pub(crate) unsafe fn support_of_context<'a>(
    context: *const rcl_context_t,
) -> Option<&'a rclc_support_t> {
    let ctx = unsafe { context.as_ref() }?;
    let support = unsafe { ctx._support.as_ref() }?;
    if !core::ptr::eq(core::ptr::addr_of!(support.context), context)
        || support.state != rclc_support_state_t::RCLC_SUPPORT_STATE_INITIALIZED
    {
        return None;
    }
    Some(support)
}

/// Support context structure.
///
/// This is the main context for nros, similar to rclc_support_t.
/// It manages the middleware session and provides shared resources.
#[repr(C)]
pub struct rclc_support_t {
    /// phase-492 W1.b — rclc's `support.context`, the argument
    /// `rclc_executor_init(&executor, &support.context, n, &allocator)` takes.
    /// Names this support; set by init, cleared by fini.
    pub context: rcl_context_t,
    /// Current state
    pub state: rclc_support_state_t,
    /// Domain ID (ROS_DOMAIN_ID)
    pub domain_id: u8,
    /// Locator string storage
    pub locator: [u8; MAX_LOCATOR_LEN],
    /// Locator string length
    pub locator_len: usize,
    /// Inline opaque storage for the RMW session.
    /// Avoids heap allocation — managed by rclc_support_init/fini.
    pub _opaque: [u64; SESSION_OPAQUE_U64S],
}

// SESSION_OPAQUE_U64S is computed from size_of::<RmwSession>() in opaque_sizes.rs —
// always large enough by construction.

impl Default for rclc_support_t {
    fn default() -> Self {
        Self {
            context: rcl_context_t {
                _support: core::ptr::null(),
            },
            state: rclc_support_state_t::RCLC_SUPPORT_STATE_UNINITIALIZED,
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
pub extern "C" fn rclc_support_get_zero_initialized() -> rclc_support_t {
    rclc_support_t::default()
}

/// Initialize the support context — rclc's signature (phase-492 W1.b).
///
/// `rclc_support_init(&support, argc, argv, &allocator)`, as an rclc program
/// writes it. The locator and domain come from the build and, on a hosted
/// target, the environment (`NROS_LOCATOR`, `ROS_DOMAIN_ID`), exactly as
/// [`rclc_support_init_with_locator`] resolves them when given NULL and 0.
///
/// The envelope: `argv` may carry program arguments, which are ignored as
/// rcl ignores non-ROS ones, but `--ros-args` is REFUSED with
/// `RCL_RET_UNSUPPORTED` — the C binding has no ROS-argument parser, and
/// dropping remaps silently would run a different graph. `allocator` must be
/// NULL or `rcl_get_default_allocator()`'s (nothing here allocates).
///
/// # Safety
/// * `support` must point to a zero-initialized `rclc_support_t`.
/// * `argv` must hold `argc` NUL-terminated strings, or be NULL with `argc` 0.
/// * `allocator` must be NULL or readable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_support_init(
    support: *mut rclc_support_t,
    argc: core::ffi::c_int,
    argv: *const *const c_char,
    allocator: *const crate::allocator::rcl_allocator_t,
) -> rcl_ret_t {
    unsafe {
        nros_c_support_init_baked(
            support,
            argc,
            argv,
            allocator,
            core::ptr::null(),
            core::ptr::null(),
            0,
        )
    }
}

/// [`rclc_support_init`] with the image's BAKED connect rungs — the seam
/// `<rclc/detail/baked_rmw.h>`'s `rclc_support_init` macro forwards to: the RMW
/// selector (`NROS_ENTRY_RMW`, issue 1531) and, since phase-499 W2, the
/// locator and domain (`NROS_ENTRY_LOCATOR` / `NROS_ENTRY_DOMAIN_ID`). Those
/// are macros defined in the CALLER's translation unit — a cargo-rooted C
/// image bakes them there (`nros_cc_flags::entry_config`) — so only a header
/// can carry them here; without this a C image calling rclc's form dialled
/// the backend default (measured: every bare-metal C cell, ConnectionFailed),
/// while C++'s inline `init_in()` read them. They are the BAKED rung: the
/// hosted environment still wins in the resolver. Not for callers: an
/// application names a backend with [`rclc_support_init_rmw`].
///
/// # Safety
/// As [`rclc_support_init`]; `rmw` and `locator` NULL or NUL-terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_c_support_init_baked(
    support: *mut rclc_support_t,
    argc: core::ffi::c_int,
    argv: *const *const c_char,
    allocator: *const crate::allocator::rcl_allocator_t,
    rmw: *const c_char,
    locator: *const c_char,
    domain_id: u8,
) -> rcl_ret_t {
    if !unsafe { crate::allocator::allocator_is_accepted(allocator) } {
        return RCL_RET_INVALID_ARGUMENT;
    }
    if argc < 0 || (argc > 0 && argv.is_null()) {
        return RCL_RET_INVALID_ARGUMENT;
    }
    for i in 0..argc as usize {
        let arg = unsafe { *argv.add(i) };
        if !arg.is_null() && unsafe { core::ffi::CStr::from_ptr(arg) }.to_bytes() == b"--ros-args" {
            return RCL_RET_UNSUPPORTED;
        }
    }
    unsafe { rclc_support_init_rmw(support, locator, domain_id, core::ptr::null(), rmw) }
}

/// Initialize the support context with an explicit locator and domain.
///
/// What `rclc_support_init` was before it took rclc's signature: the
/// middleware locator (e.g. `"tcp/127.0.0.1:7447"`, or NULL for the default)
/// and the ROS domain (0 = unset, resolved from the build / environment).
///
/// # Returns
/// * `RCL_RET_OK` on success
/// * `RCL_RET_INVALID_ARGUMENT` if support is NULL
/// * `RCL_RET_ERROR` on initialization failure
///
/// # Safety
/// * `support` must be a valid pointer to a zero-initialized rclc_support_t
/// * `locator` must be a valid null-terminated string or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_support_init_with_locator(
    support: *mut rclc_support_t,
    locator: *const c_char,
    domain_id: u8,
) -> rcl_ret_t {
    unsafe { rclc_support_init_named(support, locator, domain_id, core::ptr::null()) }
}

/// Initialize the support context with a session name.
///
/// Like `rclc_support_init`, but allows specifying a session name for
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
/// * `support` must be a valid pointer to a zero-initialized rclc_support_t
/// * `locator` and `session_name` must be valid null-terminated strings or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_support_init_named(
    support: *mut rclc_support_t,
    locator: *const c_char,
    domain_id: u8,
    session_name: *const c_char,
) -> rcl_ret_t {
    // Issue 1050 defect (3) — NULL selector: this image names no backend, so
    // the registry must hold exactly one.
    unsafe { rclc_support_init_rmw(support, locator, domain_id, session_name, core::ptr::null()) }
}

/// Issue 1050 defect (3) — [`rclc_support_init_named`] with an explicit RMW
/// selector.
///
/// `rmw` is the BAKED rung of precedence model A (RFC-0045): a hosted
/// `$NROS_RMW` still wins, NULL or `""` means "no selector", and a selector
/// that names no registered backend is an error rather than a fallback.
///
/// The C surface needed this most. Its open path
/// (`nros::internals::open_session`) never consulted `resolve_backend` at all —
/// it took registry slot 0 whenever `$NROS_RMW` was unset, so a second backend
/// registering from an `.init_array` ctor before `main` silently won. That is
/// fixed at the source (`nros_rmw_cffi::get_vtable` resolves once, for every
/// language), and this is how a C image says which one it meant.
///
/// # Safety
/// As [`rclc_support_init_named`], plus: `rmw` must be a valid NUL-terminated
/// string or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_support_init_rmw(
    support: *mut rclc_support_t,
    locator: *const c_char,
    domain_id: u8,
    session_name: *const c_char,
    rmw: *const c_char,
) -> rcl_ret_t {
    // Issue 1551 — a sink before the first Rust-side record, not after the
    // first C one (see `log::ensure_default_sinks`).
    crate::log::ensure_default_sinks();
    if support.is_null() {
        return RCL_RET_INVALID_ARGUMENT;
    }

    let support = &mut *support;

    // Check if already initialized
    if support.state != rclc_support_state_t::RCLC_SUPPORT_STATE_UNINITIALIZED {
        return RCLC_RET_BAD_SEQUENCE;
    }

    // Issue 1556 item 2 — census mode is decided BEFORE the session opens,
    // because it decides which backend opens it (the recorder, selected
    // through `$NROS_RMW`, which `resolve_boot` below reads).
    if let Err(ret) = crate::census::arm() {
        return ret;
    }

    // RFC-0045 / issue #206 — route locator + domain through the ONE
    // boot-config resolver (precedence model A: hosted env > baked >
    // default). This gives the C API the same NROS_LOCATOR/ROS_DOMAIN_ID
    // env overlay Rust and C++ have (previously: none), with the same
    // validation (malformed / > DOMAIN_ID_MAX ROS_DOMAIN_ID = error, never
    // silent 0). The BAKED rung = the caller's explicit args, falling back
    // to the historical per-backend default locator; domain_id 0 = the
    // unset sentinel (ROS convention).
    let baked_locator: Option<&str> = if !locator.is_null() {
        match core::ffi::CStr::from_ptr(locator).to_str() {
            Ok(s) => Some(s),
            Err(_) => return RCL_RET_INVALID_ARGUMENT,
        }
    } else {
        // Issue 0330 — NO backend default here. This layer is RMW-blind, and
        // the substitution that used to live here (`127.0.0.1:2019`) is
        // exactly why: it was an XRCE fact whose port had DRIFTED away from
        // the xrce backend's own `XRCE_DEFAULT_AGENT_PORT` (2018), and it was
        // applied to zenoh / dds / cyclonedds builds too. The locator stays
        // absent and the linked backend applies its own default
        // (`nros-rmw-xrce/session.c` for xrce, `nros_rmw_zenoh::DEFAULT_LOCATOR`
        // for zenoh; cyclonedds ignores the locator).
        None
    };
    let baked = nros_node::BootConfig {
        node_name: None,
        locator: baked_locator,
        // Issue #227 — NROS_DOMAIN_ID_EXPLICIT_ZERO (255) = explicit domain 0;
        // 0 stays the unset sentinel (model A / ROS convention); 233..=254
        // flow to the resolver's range check and fail loudly.
        domain_id: nros_node::baked_domain_from_c_abi(domain_id),
        namespace: None,
        // Issue 1050 defect (3) — an empty selector is "unset", not a backend
        // named "": the bake macro expands to a string literal, and an
        // unresolved cmake variable produces `""` rather than nothing.
        rmw: if rmw.is_null() {
            None
        } else {
            match core::ffi::CStr::from_ptr(rmw).to_str() {
                Ok("") => None,
                Ok(s) => Some(s),
                Err(_) => return RCL_RET_INVALID_ARGUMENT,
            }
        },
    };
    // issue 0687 — the environment rung comes from the hosted edge now. This
    // call passed `hosted_env: true` unconditionally and relied on the core's
    // `env` cfg to make it inert on a target build; the cfg is HERE now, where
    // the capability is, so a no-`env` C image says so instead of asking for a
    // rung that cannot exist.
    let resolved = match resolve_boot(baked) {
        Ok(cfg) => cfg,
        Err(_) => return RCL_RET_INVALID_ARGUMENT,
    };

    // Store the RESOLVED domain ID (fits u8: try_resolve enforces
    // DOMAIN_ID_MAX = 232).
    support.domain_id = resolved.domain_id as u8;

    // Store the RESOLVED locator.
    {
        let bytes = resolved.locator.as_bytes();
        let cap = support.locator.len() - 1;
        if bytes.len() > cap {
            return RCL_RET_INVALID_ARGUMENT;
        }
        support.locator[..bytes.len()].copy_from_slice(bytes);
        support.locator[bytes.len()] = 0;
        support.locator_len = bytes.len();
    }

    // Phase 128.C.2 — RMW-blind support init. Every linked backend
    // self-registers via the `RMW_INIT_ENTRIES` linker section
    // (linkme on the Rust side, `NROS_RMW_REGISTER_BACKEND` macro on
    // the C/C++ side). The walker fires inside the matching
    // `nros::internals::open_session` / `Executor::open` call below;
    // the prior `nros_app_register_backends()` weak/strong dance and
    // the legacy `cffi-xrce-c` hook are no longer needed.
    //
    // Phase 155.B.4 — Phase 128.C.2's "no longer needed" assumption
    // doesn't hold on RTOS targets `linkme` doesn't recognise:
    // FreeRTOS, NuttX, Zephyr, ESP-IDF all silently skip the
    // distributed-slice entry, so `walk_init_section` finds zero
    // backends and `get_vtable()` returns
    // `TransportError::InvalidArgument` (registry empty). The
    // sibling `nros_cpp_init` still calls
    // `nros_app_register_backends()` explicitly for this exact
    // reason. Mirror that here so the C-API path works on the
    // same RTOSes.
    #[cfg(feature = "rmw-cffi")]
    {
        unsafe extern "C" {
            fn nros_app_register_backends();
        }
        unsafe {
            nros_app_register_backends();
        }
    }

    // Initialize the middleware session
    #[cfg(feature = "rmw-cffi")]
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
            // Phase 266 (W5b) — null/empty name defaults to "node" (unified
            // compiled default across C, C++, and Rust, matching the Rust
            // resolver compiled default). The old `nros_{pid}` / "nros"
            // fallback is replaced for consistent `ros2 node list` output.
            nros_core::heapless::String::try_from("node").unwrap()
        };

        match nros::internals::open_session(
            locator_str,
            SessionMode::Client,
            support.domain_id as u32,
            &name,
            // Issue 1050 defect (3) — the RESOLVED selector (env > baked). The
            // borrow is of `resolved`, which outlives this call.
            resolved.rmw,
        ) {
            Ok(session) => {
                // Write session directly into inline opaque storage
                core::ptr::write(
                    support._opaque.as_mut_ptr() as *mut nros::internals::RmwSession,
                    session,
                );
                support.state = rclc_support_state_t::RCLC_SUPPORT_STATE_INITIALIZED;
                support.context._support = support as *const rclc_support_t;
                RCL_RET_OK
            }
            // Phase 155.B — surface the inner `TransportError` variant
            // as a specific `NROS_RET_*` code so a fresh
            // `rclc_support_init -> -X` log line tells the user which
            // precondition the backend rejected, instead of every
            // failure mode collapsing to RCL_RET_ERROR (-1).
            Err(e) => transport_error_to_ret(e),
        }
    }

    #[cfg(not(feature = "rmw-cffi"))]
    {
        RCL_RET_ERROR
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
/// * `RCL_RET_OK` on success
/// * `RCL_RET_INVALID_ARGUMENT` if support is NULL
/// * `RCL_RET_NOT_INIT` if not initialized
///
/// # Safety
/// * `support` must be a valid pointer to an initialized rclc_support_t
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_support_fini(support: *mut rclc_support_t) -> rcl_ret_t {
    if support.is_null() {
        return RCL_RET_INVALID_ARGUMENT;
    }

    let support = &mut *support;

    if support.state != rclc_support_state_t::RCLC_SUPPORT_STATE_INITIALIZED {
        return RCL_RET_NOT_INIT;
    }

    // Drop the inline RMW session
    #[cfg(feature = "rmw-cffi")]
    {
        core::ptr::drop_in_place(support._opaque.as_mut_ptr() as *mut nros::internals::RmwSession);
    }

    support._opaque = [0u64; SESSION_OPAQUE_U64S];
    support.state = rclc_support_state_t::RCLC_SUPPORT_STATE_SHUTDOWN;
    support.context._support = core::ptr::null();

    RCL_RET_OK
}

/// Check if support context is valid (initialized).
///
/// # Parameters
/// * `support` - Pointer to a support context
///
/// # Returns
/// * `true` if valid, `false` if invalid or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_support_is_valid(support: *const rclc_support_t) -> bool {
    if support.is_null() {
        return false;
    }

    let support = &*support;
    support.state == rclc_support_state_t::RCLC_SUPPORT_STATE_INITIALIZED
}

#[cfg(kani)]
mod verification {
    use super::*;
    use crate::error::*;

    #[kani::proof]
    #[kani::unwind(5)]
    fn support_init_null_ptr() {
        // NULL support pointer → INVALID_ARGUMENT
        let ret =
            unsafe { rclc_support_init_with_locator(core::ptr::null_mut(), core::ptr::null(), 0) };
        assert_eq!(ret, RCL_RET_INVALID_ARGUMENT);
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn support_zero_initialized_state() {
        let support = rclc_support_get_zero_initialized();
        assert_eq!(
            support.state,
            rclc_support_state_t::RCLC_SUPPORT_STATE_UNINITIALIZED
        );
        assert_eq!(support.domain_id, 0);
        assert!(support._opaque.iter().all(|&v| v == 0));
    }
}

impl rclc_support_t {
    /// Get the raw session pointer (for executor initialization).
    pub(crate) fn get_session_ptr(&self) -> *mut nros::internals::RmwSession {
        self._opaque.as_ptr() as *mut nros::internals::RmwSession
    }

    /// Get the internal session reference (for internal use)
    pub(crate) unsafe fn get_session(&self) -> Option<&nros::internals::RmwSession> {
        if self.state != rclc_support_state_t::RCLC_SUPPORT_STATE_INITIALIZED {
            None
        } else {
            Some(&*(self._opaque.as_ptr() as *const nros::internals::RmwSession))
        }
    }

    /// Get the internal session reference mutably (for internal use)
    pub(crate) unsafe fn get_session_mut(&mut self) -> Option<&mut nros::internals::RmwSession> {
        if self.state != rclc_support_state_t::RCLC_SUPPORT_STATE_INITIALIZED {
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

/// Resolve boot config, with the environment rung when this build has the
/// `env` capability.
///
/// issue 0687 — `ExecutorConfig::try_resolve` used to take `hosted_env: bool`
/// and read the process environment inside the core crate. The core takes
/// values now, so the cfg that decides whether there IS an environment lives at
/// the edge that has one.
fn resolve_boot(
    baked: nros_node::BootConfig<'_>,
) -> Result<nros_node::ExecutorConfig<'_>, nros_node::BootConfigError> {
    // Issue 1763 / phase-359 W10 stage B — every image inside a host process
    // has an environment, `std` or not (threadx-linux, freertos-posix), and
    // the one reader takes it through the C library's `getenv`.
    #[cfg(target_os = "linux")]
    {
        nros::env::try_resolve_hosted(baked)
    }
    #[cfg(not(target_os = "linux"))]
    {
        nros_node::ExecutorConfig::try_resolve(baked)
    }
}
